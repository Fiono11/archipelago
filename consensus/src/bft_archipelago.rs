use std::{cmp::max, collections::{BTreeSet, HashMap, HashSet}, sync::{atomic::{AtomicBool, Ordering}, mpsc::{Receiver, Sender}, Arc, RwLock}, thread};
use crate::{AValue, BValue, Broadcast, BroadcastHash, Decision, Id, Message, RValue, Rank, Response, State, Step, Value};
use rand::{self, Rng};

// Each process receives 2f+1 responses per step and rank 
type Responses = Arc<RwLock<HashMap<(Step, Rank), HashMap<Id, Response>>>>;

// In the first step of rank i, each process: 
// 1) Broadcasts its rank i, value v and an optional certificate containing responses of step B and rank i-1 from 2f+1 processes (if i > 0)
// 2) Waits valid responses from 2f+1 processes 
// 3) Keeps the maximum RValue v'
// 4) Returns (i, v')
type R = Arc<RwLock<RValue>>;

// In the second step of rank i, each process: 
// 1) Broadcasts its rank i, value v and a certificate containing responses of step R and rank i from 2f+1 processes 
// 2) Waits valid responses from 2f+1 processes 
// 3) Keeps the the greatest AValue or the two greatest if there are different values
// 4) According to the received AResponses, it returns:
// - (true, v) if there is only one Avalue v 
// - (false, max(v)), otherwise
type A = Arc<RwLock<Vec<AValue>>>;

// In the third step of rank i, each process: 
// 1) Broadcasts its rank i, value v, a boolean flag and a certificate containing responses of step R and rank i from 2f+1 processes 
// 2) Waits valid responses from 2f+1 processes
// 3) Keeps the (true, v) pair and the highest (false, pair), if there is one
// 4) According to the received BResponses, it returns:
// - (commit, v) if there are at least 2f+1 (commit, v)
// - (adopt, v) if there is at least 1 (commit, v)
// - (adopt, max(v)) otherwise
type B = Arc<RwLock<Vec<BValue>>>;

// Maps broadcasts to their count
type Broadcasts = HashMap<Broadcast, i64>;

// Maps responses to their states
type PendingResponses = HashMap<BTreeSet<BroadcastHash>, HashSet<Response>>;

#[derive(Debug, Clone)]
pub struct Process {
    id: Id,
    responses: Responses,
    senders: Vec<Sender<Message>>,
    stop_flag: Arc<AtomicBool>,
    byzantine: bool,
}

impl Process {
    pub fn new(id: Id, f: usize, senders: Vec<Sender<Message>>, receiver: Receiver<Message>, byzantine: bool) -> Self {   
        let responses = Arc::new(RwLock::new(HashMap::new()));
        let responses_clone = responses.clone();
        let senders_clone = senders.clone();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = Arc::clone(&stop_flag);

        let state = Process {
            id,
            responses,
            senders,
            stop_flag,
            byzantine
        };
                
        // Start message handling in a background thread
        thread::spawn(move || {
            Process::run(
                id,
                f,
                responses_clone,
                senders_clone,
                stop_flag_clone,
                receiver,
                byzantine
            );
        });
        
        state
    }

    fn run(
        id: Id,
        f: usize,
        responses: Responses,
        senders: Vec<Sender<Message>>,
        stop_flag: Arc<AtomicBool>,
        receiver: Receiver<Message>,
        byzantine: bool
    ) {
        let r_set = Arc::new(RwLock::new(RValue::default()));
        let a_sets = Arc::new(RwLock::new(Vec::new()));
        let b_sets = Arc::new(RwLock::new(Vec::new()));
        let mut broadcasts: Broadcasts = HashMap::new();
        let mut pending_responses: PendingResponses = HashMap::new();

        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            if let Ok(msg) = receiver.recv() {
                match msg {
                    Message::Broadcast(broadcast) => {                        
                        // Lines 26, 42, 62
                        let is_reliable = Process::reliably_check_broadcast(&broadcast, &broadcasts, f);

                        if is_reliable {
                            broadcasts.entry(broadcast.clone()).or_insert(0);

                            match broadcast.step {
                                Step::R => {
                                    Process::answer_r_broadcast(
                                        id,
                                        &broadcast,
                                        &senders,
                                        &r_set,
                                        &broadcasts,
                                        byzantine
                                    );
                                }
                                Step::A => {
                                    Process::answer_a_broadcast(
                                        id,
                                        &broadcast,
                                        &senders,
                                        &a_sets,
                                        &broadcasts,
                                        byzantine
                                    );
                                }
                                Step::B => {
                                    Process::answer_b_broadcast(
                                        id,
                                        &broadcast,
                                        &senders,
                                        &b_sets,
                                        &broadcasts,
                                        byzantine
                                    );
                                }
                            }
                        }          
                    }
                    Message::Response(response) => {
                        Process::reliably_check_response(
                            response,
                            &responses,
                            &mut pending_responses,
                            2 * f + 1
                        );
                    }
                }
            }
        }
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    fn send_message(senders: &[Sender<Message>], message: &mut Message, byzantine: bool) {   
        if byzantine {
            Process::apply_byzantine_behavior(message);
        }
            
        for sender in senders {
            sender.send(message.clone()).unwrap_or_else(|e| {
                eprintln!("Failed to send message: {}", e);
            });
        }
    }

    fn apply_byzantine_behavior(message: &mut Message) {
        let mut rng = rand::thread_rng();
        match message {
            Message::Broadcast(broadcast) => {
                match rng.gen_range(0..3) {
                    0 => broadcast.step = match broadcast.step {
                        Step::R => Step::A,
                        Step::A => Step::B,
                        Step::B => Step::R,
                    },
                    1 => broadcast.rank = rng.gen_range(1..3),
                    2 => broadcast.flag = Some(rng.gen_bool(0.5)),
                    _ => unreachable!(),
                }
            }
       
            Message::Response(response) => {
                match rng.gen_range(0..2) {
                    0 => response.step = match response.step {
                        Step::R => Step::A,
                        Step::A => Step::B,
                        Step::B => Step::R,
                    },
                    1 => response.rank = rng.gen_range(1..3),
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn propose(&mut self, threshold: usize, value: i64, rank: Rank) -> i64 {
        loop {
            if self.stop_flag.load(Ordering::Relaxed) {
                return -1;
            }

            let r_value = self.r_step(threshold, RValue::new(rank, value));

            let (flag, a_value) = self.a_step(threshold, r_value);

            let decision = self.b_step(threshold, r_value.rank, flag, a_value);
            
            match decision {
                Decision::Commit(val) => return val,
                Decision::Adopt(val) => self.propose(threshold, val, rank + 1)
            };
        }
    }

    // Line 15: procedure R-Step(v)
    fn r_step(&mut self, threshold: usize, r_value: RValue) -> RValue {
        let rank = r_value.rank;
        let value = r_value.value;

        // Line 16: compile certificate C (empty at rank 0) 
        // Line 90: To compile a broadcast certificate, list all 2f + 1 answers to the previous step broadcast received during the previous step.
        // Line 17: broadcast(R, i, v, C) 
        if rank > 0 {
            let key = (Step::B, rank - 1);

            loop {
                let responses = self.responses.read().unwrap();
                if responses.get(&key).map(|m| m.len()).unwrap_or(0) == threshold {
                    break;
                }
            }
                                
            let responses = self.responses.read().unwrap().get(&key).unwrap().values().cloned().collect();
                    
            let broadcast = Broadcast::new(self.id, Step::R, value, None, rank, Some(responses));
            
            Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        }
        else {
            let broadcast = Broadcast::new(self.id, Step::R, r_value.value, None, r_value.rank, None);
                
            Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        }

        let key = (Step::R, rank);

        // Line 18/19: wait until (receive valid (Rresp, i, R, C) from 2f + 1 processes)
        loop {
            let responses = self.responses.read().unwrap();
            if responses.get(&key).map(|m| m.len()).unwrap_or(0) == threshold {
                let response_vec = responses.get(&key)
                    .unwrap()
                    .values()
                    .cloned()
                    .collect::<Vec<Response>>();

                let max_value = Self::process_r_responses(&response_vec);
                
                // Line 22: R ← max(R)
                return max_value;
            }
        }
    }

    fn process_r_responses(responses: &[Response]) -> RValue {
        // Line 20: R ← union of all valid Rs received in previous line (the paper has a typo?)
        let r_values: Vec<RValue> = responses
            .iter()
            .filter_map(|response| {
                for state in &response.state {
                    if let Value::RValue(r_value) = state.value {
                        return Some(r_value);
                    }
                }
                None
            })
            .collect();

        // Line 21: ⟨i’,v’⟩ ← max(R)
        *r_values.iter().max().unwrap()
    }

    // Line 25: Upon delivering (R, j, v, C) from p
    fn answer_r_broadcast(
        id: Id,
        broadcast: &Broadcast,
        senders: &[Sender<Message>],
        r_set: &R,
        broadcasts: &Broadcasts,
        byzantine: bool
    ) {
        let broadcast_r_value = RValue::new(broadcast.rank, broadcast.value);

        // Line 27: R ← max(⟨j, v⟩, R)
        let max_r_value = max(broadcast_r_value, *r_set.read().unwrap());
        
        {
            *r_set.write().unwrap() = max_r_value;
        }
        
        // Line 28: b ← bcast responsible for R’s value (the paper has a typo?)
        let response_broadcast = {
            broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == max_r_value.value && rb.rank == max_r_value.rank && rb.step == broadcast.step))
                .map(|(b, _)| b.clone())
                .unwrap()
        };

        // Page 9: A broadcast from pi justifies a response from pj for an R-Step if it contains the highest value encountered that appears in pj response.
        let response = Response::new(
            id,
            Step::R,
            broadcast.rank,
            vec![State::new(Value::RValue(max_r_value), response_broadcast)],
        );

        // Line 29: send(Rresp, j, R, sig, b) to all
        Process::send_message(senders, &mut Message::Response(response), byzantine);
    }

    // Line 31: Procedure A-Step(i, v)
    fn a_step(&mut self, threshold: usize, r_value: RValue) -> (bool, i64) {
        let value = r_value.value;
        let rank = r_value.rank;

        let key = (Step::R, rank);

        // Line 32: compile certificate C
        let responses = self.responses.read().unwrap().get(&key).unwrap().values().cloned().collect();
        
        let broadcast = Broadcast::new(self.id, Step::A, value, None, rank, Some(responses));

        // Line 33: broadcast(A, i, v, C)
        Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        
        let key = (Step::A, rank);

        // Lines 34/35:  wait until receive valid (Aresp, i, A[i]) 35: from 2f + 1 processes
        loop {
            let responses = self.responses.read().unwrap();
            if responses.get(&key).map(|m| m.len()).unwrap_or(0) == threshold {
                let response_vec = responses.get(&key)
                    .unwrap()
                    .values()
                    .cloned()
                    .collect::<Vec<Response>>();
                
                return Self::process_a_responses(&response_vec, threshold);
            }
        }
    }

    fn process_a_responses(responses: &[Response], threshold: usize) -> (bool, i64) {
        // Line 36: S ← union of all A[i]s received
        let a_values: Vec<AValue> = responses
            .iter()
            .filter_map(|response| {
                for state in &response.state {
                    if let Value::AValue(a_value) = state.value {
                        return Some(a_value);
                    }
                }
                None
            })
            .collect();
        
        let mut value_counts: HashMap<AValue, usize> = HashMap::new();
        let mut max_value = a_values.first().map_or(AValue(0), |v| *v);

        for a_value in &a_values {
            *value_counts.entry(*a_value).or_insert(0) += 1;            
            if a_value.0 > max_value.0 {
                max_value = *a_value;
            }
        }

        // Line 37/38: if (S contains at least 2f+1 A-answers containing only val)
        for (val, count) in value_counts.iter() {
            if *count >= threshold {
                // Line 39: return ⟨true, val⟩
                return (true, val.0);
            }
        }

        // Line 40: else return ⟨false, max(S)⟩
        (false, max_value.0)
    }

    fn answer_a_broadcast(
        id: Id,
        broadcast: &Broadcast,
        senders: &[Sender<Message>],
        a_sets: &A,
        broadcasts: &Broadcasts,
        byzantine: bool
    ) {
        let j = broadcast.rank as usize;
        let broadcast_value = AValue(broadcast.value);
        
        {
            let mut a_sets_write = a_sets.write().unwrap();
            if a_sets_write.len() > j {
                // Line 43: if v /∈ A[j] and |A[j]| < 2
                if !a_sets_write.contains(&broadcast_value) && a_sets_write.len() < 2 {
                    // Line 44: add v to A[j]
                    a_sets_write.push(broadcast_value);
                // Line 45: v > max(A[j])
                } else if broadcast_value > *a_sets_write.iter().max().unwrap() {
                    let min = a_sets_write.iter().min().unwrap();
                    if let Some(index) = a_sets_write.iter().position(|&x| x == *min) {
                        // Line 46: min(A[j]) ← v
                        a_sets_write[index] = broadcast_value;
                    }
                }
            }
            else {
                a_sets_write.push(broadcast_value);
            }
        }

        let mut sent_values = HashSet::new();
        
        let current_a_sets = {
            a_sets.read().unwrap().clone().clone()
        };

        /* Page 9: A broadcast from pi justifies a response from pj for an A-Step, if it contains 
        the highest value v and, if possible, 
        any value from the response different from v. */

        let mut a_states = Vec::new();
        
        for a_state in current_a_sets.iter() {
            if sent_values.insert(a_state.0) {
                // Line 47: b ← bcast responsible for A[j]’s value
                let response_broadcast = {
                    broadcasts
                        .iter()
                        .find(|(b, _)| matches!(b, rb if rb.value == a_state.0 && rb.rank == broadcast.rank && rb.step == broadcast.step))
                        .map(|(b, _)| b.clone())
                };

                if let Some(response_broadcast) = response_broadcast {
                    a_states.push(State::new(Value::AValue(*a_state), response_broadcast.clone()));
                }
            }
        }

        let response = Response::new(
            id,
            Step::A,
            broadcast.rank,
            a_states,
        );
        
        // Line 48: send(Aresp, j, A[j], sig, b) to all
        Process::send_message(senders, &mut Message::Response(response), byzantine);
    }

    fn b_step(&mut self, threshold: usize, rank: Rank, flag: bool, value: i64) -> Decision {
        // Line 51: compile certificate C
        let key = (Step::A, rank);
                
        let responses = self.responses.read().unwrap().get(&key).unwrap().values().cloned().collect();
        
        let broadcast = Broadcast::new(self.id, Step::B, value, Some(flag), rank, Some(responses));
        
        // Line 52: broadcast(B, i, , v, C)
        Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        
        let key = (Step::B, rank);

        // Line 53/54:  wait until receive valid (Bresp, i, B[i]) from 2f + 1 proc.
        loop {
            let responses = self.responses.read().unwrap();
            if responses.get(&key).map(|m| m.len()).unwrap_or(0) == threshold {
                let response_vec = responses.get(&key)
                    .unwrap()
                    .values()
                    .cloned()
                    .collect::<Vec<Response>>();
                
                return Self::process_b_responses(&response_vec, threshold);
            }
        } 
    }

    fn process_b_responses(responses: &[Response], threshold: usize) -> Decision {
        // Line 55: S ← array with all B[i]s received
        let b_values: Vec<BValue> = responses
            .iter()
            .filter_map(|response| {
                for state in &response.state {
                    if let Value::BValue(b_value) = state.value {
                        return Some(b_value);
                    }
                }
                None
            })
            .collect();

        let true_values: Vec<&BValue> = b_values.iter()
            .filter(|&b_value| b_value.flag)
            .collect();
        
        // Line 56: if |{⟨true, val⟩ ∈ S}| ≥ 2f + 1
        if true_values.len() >= threshold {
            let value = true_values.first().unwrap().value;

            // Line 57: return ⟨commit, val⟩
            Decision::Commit(value)
        }
        // Line 58: else if |{⟨true, val⟩ ∈ S}| ≥ 1 then
        else if !true_values.is_empty() {
            let value = true_values.first().unwrap().value;
            
            // Line 59: return ⟨adopt, val⟩
            Decision::Adopt(value)
        }
        else {
            let max_value = b_values.iter()
                .max_by_key(|b_value| b_value.value)
                .map(|b_value| b_value.value)
                .unwrap();

            // Line 60: else return ⟨adopt, max(S)⟩
            Decision::Adopt(max_value)
        }
    }

    fn answer_b_broadcast(
        id: Id,
        broadcast: &Broadcast,
        senders: &[Sender<Message>],
        b_sets: &B,
        broadcasts: &Broadcasts,
        byzantine: bool
    ) {
        let len = {
            b_sets.read().unwrap().len()
        };
        let j = broadcast.rank as usize;

        let value = broadcast.value;
        let flag = broadcast.flag.unwrap();
        let b_value = BValue::new(value, flag);
        let mut b_values = b_sets.write().unwrap();
        
        if len > j {
            let len = b_values.len();

            // Line 63: m ← max(B[j][0].v, B[j][1].v)
            let m = match len {
                0 => 0,
                1 => b_values[0].value,
                _ => max(b_values[0].value, b_values[1].value)
            };

            if len < 2 {
                {
                    // Line 64: if |B[j]| < 2 then add ⟨bool, v⟩ to B[j]
                    b_values.push(b_value);
                }
            }
            else {
                let contains_flag_value = {
                    b_values.iter().any(|value| value == &b_value)
                };
                
                // Lines 65/66: else if(flag ∧ ⟨flag, v⟩ ∈/ B[j] ∨ ¬flag ∧ v > m) then
                if (flag && !contains_flag_value) || (!flag && value > m) {
                    // Line 67: B[j][0] ← ⟨flag, v⟩
                    {
                        b_values[0] = b_value;
                    }
                }
            }
        }
        else {
            {
                b_values.push(b_value);
            }
        }

        /* Page 9: For a broadcast from pi to justify a response from pj for a B-Step, it must ensures the following: 
        if the response contains only true, then the broadcast should contain true; 
        if the response contains at least one true and false pair, then the broadcast should contain the true pair, and any of the false pairs; 
        if the response contains only false pairs, then the broadcast should contain the pair among them with the highest value. */
        
        let true_pairs: Vec<&BValue> = b_values.iter()
            .filter(|b_state| b_state.flag)
            .collect();
        
        let false_pairs: Vec<&BValue> = b_values.iter()
            .filter(|b_state| !b_state.flag)
            .collect();
        
        if !true_pairs.is_empty() && false_pairs.is_empty() {
            let b_value = *true_pairs[0];

            let response_broadcast = broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == b_value.value && rb.rank == broadcast.rank && rb.step == broadcast.step && rb.flag == broadcast.flag))
                .unwrap()
                .0
                .clone();

            let response = Response::new(
                id, 
                Step::B,
                broadcast.rank, 
                vec![State::new(Value::BValue(b_value), response_broadcast)], 
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        }
        else if !true_pairs.is_empty() && !false_pairs.is_empty() {
            let mut b_state = Vec::new();

            let b_value_true = *true_pairs[0];

            let response_broadcast_true = broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == b_value_true.value && rb.rank == broadcast.rank && rb.step == broadcast.step && rb.flag == broadcast.flag))
                .unwrap()
                .0
                .clone();

            b_state.push(State::new(Value::BValue(b_value_true), response_broadcast_true));
        
            let b_value_false = *false_pairs.iter()
                .max_by_key(|b_state| b_state.value)
                .unwrap();

            let response_broadcast_false = broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == b_value_false.value && rb.rank == broadcast.rank && rb.step == broadcast.step && rb.flag == broadcast.flag))
                .unwrap()
                .0
                .clone();

            b_state.push(State::new(Value::BValue(*b_value_false), response_broadcast_false));

            let response = Response::new(
                id, 
                Step::B,
                broadcast.rank, 
                b_state, 
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        }
        else if true_pairs.is_empty() && !false_pairs.is_empty() {                                        
            let highest_false = false_pairs.iter()
                .max_by_key(|b_state| b_state.value)
                .unwrap();

            let response_broadcast = broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == highest_false.value && rb.rank == broadcast.rank && rb.step == broadcast.step && rb.flag == broadcast.flag))
                .unwrap()
                .0
                .clone();
            
            let response = Response::new(
                id, 
                Step::B,
                broadcast.rank, 
                vec![State::new(Value::BValue(**highest_false), response_broadcast)], 
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        }
    }

    fn validate_response(response: &Response) -> bool {
        for state in &response.state {
            let broadcast = &state.broadcast;

            // The broadcast's step must match the response's step
            if broadcast.step != response.step {
                return false;
            }
            
            // The broadcast's rank must match the response's rank
            if broadcast.rank != response.rank {
                return false;
            }
            
            match response.step {
                Step::R => {
                    if broadcast.step != Step::R || broadcast.rank != response.rank {
                        return false;
                    }
                }
                Step::A => {
                    if broadcast.step != Step::A || broadcast.rank != response.rank {
                        return false;
                    }
                }
                Step::B => {
                    if broadcast.step != Step::B || broadcast.rank != response.rank {
                        return false;
                    }
                }
            }
        }
        true
    }

    // Line 91: To reliably check response (check if a response is valid), check if, for the broadcast(s) originating its value we have received 2f + 1 responses to that broadcast
    fn reliably_check_response(
        response: Response,
        responses: &Responses,
        pending_responses: &mut PendingResponses,
        threshold: usize
    ) {
        if !Process::validate_response(&response) {
            return;
        }
              
        let broadcast_hashes: BTreeSet<BroadcastHash> = response.state.iter()
            .map(|r| r.broadcast.hash_value())
            .collect();

        if !pending_responses.contains_key(&broadcast_hashes) {
            let mut responses_set = HashSet::new();
            responses_set.insert(response);
            pending_responses.insert(broadcast_hashes.clone(), responses_set);
        } else {
            pending_responses.get_mut(&broadcast_hashes).unwrap().insert(response);
        }

        if let Some(received_responses) = pending_responses.get(&broadcast_hashes) {                
            if received_responses.len() >= threshold {                    
                for resp in received_responses {
                    let key = (resp.step, resp.rank);
                    let mut responses_map = responses.write().unwrap();
                    let entry = responses_map.entry(key).or_default();
                    
                    if !entry.contains_key(&resp.sender) && entry.len() < threshold {
                        entry.insert(resp.sender, resp.clone());
                    }
                }
            }
        }
    }

    fn reliably_check_broadcast(
        broadcast: &Broadcast,
        broadcasts: &Broadcasts,
        f: usize,
    ) -> bool {
        if broadcast.step == Step::R && broadcast.rank == 0 {
            return true;
        }

        if broadcast.previous_step_responses.is_none() {
            return false;
        }

        let threshold = 2 * f + 1;
        let responses = broadcast.previous_step_responses.as_ref().unwrap();

        // Lines 74/75: if |{bcast-answers ∈ C}| > f then return true
        // If at least f+1 responses contain this broadcast, it means that at least one of those response comes from a correct process, 
        // which reliably checked the broadcast, so we don't have to check itå
        if *broadcasts.get(broadcast).unwrap_or(&0) as usize > f {
            return true;
        }

        // Line 76: check that |C| ≥ 2f + 1 messages 
        if responses.len() < threshold {
            return false;
        }
        
        // Missing
        // Line 77: check signatures of those messages 
        // Line 78: check if |{bcast-answers }| > f

        match broadcast.step {
            // Lines 79/80/81: If X = R then check (i, v) is correct according to signed B-answers received and step B
            Step::R => {
                if broadcast.flag.is_some() {
                    false
                }
                else if broadcast.rank == 0 {
                    true
                } else {
                    Process::process_b_responses(responses, threshold) == Decision::Adopt(broadcast.value)
                }
            }
            // Lines 82/83/84: else if X=A then	check (i, v) is correct according to signed R-answers received and step R
            Step::A => {
                if broadcast.flag.is_some() {
                    false
                }
                else {
                    Process::process_r_responses(responses).value == broadcast.value
                }
            }
            // Lines 85/86/87: else if X= B then check (i, bool, v) is correct according to signed A-answers received and step A
            Step::B => {
                if broadcast.flag.is_none() {
                    false
                }
                else {
                    Process::process_a_responses(responses, threshold) == (broadcast.flag.unwrap(), broadcast.value)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc::channel, thread};
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn setup_logger() {
        INIT.call_once(|| {
            env_logger::Builder::from_default_env()
                .format_timestamp_secs()
                .init();
        });
    }

    #[test]
    fn test_consensus() {
        setup_logger();

        for instance in 1..1000 {
            let (sender1, receiver1) = channel();
            let (sender2, receiver2) = channel();
            let (sender3, receiver3) = channel();
            let (sender4, receiver4) = channel();

            let f: usize = 1;
            let threshold = 2 * f + 1;
            
            let mut process1 = Process::new(0, f, vec![sender1.clone(), sender2.clone(), sender3.clone(), sender4.clone()], receiver1, false);
            let mut process1_clone = process1.clone();
            
            let mut process2 = Process::new(1, f, vec![sender1.clone(), sender2.clone(), sender3.clone(), sender4.clone()], receiver2, false);
            let mut process2_clone = process2.clone();
            
            let mut process3 = Process::new(2, f, vec![sender1.clone(), sender2.clone(), sender3.clone(), sender4.clone()], receiver3, false);
            let mut process3_clone = process3.clone();
            
            let mut process4 = Process::new(3, f, vec![sender1.clone(), sender2.clone(), sender3.clone(), sender4.clone()], receiver4, true);
            let mut process4_clone = process4.clone();

            let p1 = thread::spawn(move || {
                process1.propose(threshold, instance + 0, 0)
            });
            
            let p2 = thread::spawn(move || {
                process2.propose(threshold, instance + 1, 0)
            });
            
            let p3 = thread::spawn(move || {
                process3.propose(threshold, instance + 2, 0)
            });

            let p4 = thread::spawn(move || {
                process4.propose(threshold, instance + 3, 0)
            });
            
            let p1_value = p1.join().unwrap();
            let p2_value = p2.join().unwrap();
            let p3_value = p3.join().unwrap();
            let _ = p4.join().unwrap();

            process1_clone.stop();
            process2_clone.stop();
            process3_clone.stop();
            process4_clone.stop();

            assert_eq!(p1_value, p2_value);
            assert_eq!(p1_value, p3_value);
        }
    }
}