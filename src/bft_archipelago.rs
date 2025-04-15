use std::{cmp::max, collections::{BTreeSet, HashMap, HashSet}, sync::{atomic::{AtomicBool, Ordering}, mpsc::{Receiver, Sender}, Arc, RwLock}, thread};
use crate::{AValue, BValue, Broadcast, BroadcastHash, Decision, Id, Message, RValue, Rank, Response, State, Step, Value};
use log::info;
use rand::{self, Rng};

// Each process receives 2f+1 responses per step and rank 
type Responses = Arc<RwLock<HashMap<(Step, Rank), HashMap<Id, Response>>>>;

// In the first step of rank i, each process: 
// 1) Broadcasts its R
// 2) Waits valid responses from 2f+1 processes 
// 3) Keeps the maximum value v in R
// 4) Returns (i, v)
type R = Arc<RwLock<RValue>>;

// In the second step of rank i, each process: 
// 1) Broadcasts its A
// 2) Waits valid responses from 2f+1 processes 
// 3) Keeps the two highest values in A
// 4) According to the received AResponses, it returns:
// - (true, v) if there is only one value v 
// - (false, max(A)) otherwise
type A = Arc<RwLock<Vec<AValue>>>;

// In the third step of rank i, each process: 
// 1) Broadcasts its B
// 2) Waits valid responses from 2f+1 processes
// 3) Keeps the (true, v) pair and the highest (false, pair), if there is one
// 4) According to the received BResponses, it returns:
// - (commit, v) if there are at least 2f+1 (commit, v)
// - (adopt, v) if there is at least 1 (commit, v)
// - (adopt, max(BResponses)) otherwise
type B = Arc<RwLock<Vec<BValue>>>;

// Maps broadcasts to their count
type Broadcasts = HashMap<Broadcast, i64>;

// Maps responses to their states
type PendingResponses = HashMap<BTreeSet<BroadcastHash>, HashSet<Response>>;

#[derive(Debug, Clone)]
pub struct Process {
    id: Id,
    r_set: R,
    a_sets: A,
    b_sets: B,
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
        let r_set = Arc::new(RwLock::new(RValue::default()));
        let a_sets = Arc::new(RwLock::new(Vec::new()));
        let b_sets = Arc::new(RwLock::new(Vec::new()));
        let r_set_clone = Arc::clone(&r_set);
        let a_sets_clone = Arc::clone(&a_sets);
        let b_sets_clone = Arc::clone(&b_sets);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = Arc::clone(&stop_flag);

        let state = Process {
            id,
            r_set,
            a_sets,
            b_sets,
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
                r_set_clone,
                a_sets_clone,
                b_sets_clone,
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
        r_set: R,
        a_sets: A,
        b_sets: B,
        stop_flag: Arc<AtomicBool>,
        receiver: Receiver<Message>,
        byzantine: bool
    ) {
        let mut broadcasts: Broadcasts = HashMap::new();
        let mut pending_responses: PendingResponses = HashMap::new();

        loop {
            // Check if we should stop
            if stop_flag.load(Ordering::Relaxed) {
                info!("Process {} received stop signal, terminating", id);
                break;
            }

            if let Ok(msg) = receiver.recv() {
                match msg {
                    Message::Broadcast(broadcast) => {                        
                        info!("Process {} RECEIVED BROADCAST with hash {}: step={:?}, sender={}, rank={}, value={}, previous_step_responses={:?}", 
                            id, broadcast.hash_value(), broadcast.step, broadcast.sender, broadcast.rank, broadcast.value, broadcast.previous_step_responses);

                        // Lines 26, 42, 62
                        let is_reliable = Process::reliably_check_broadcast(&broadcast, &broadcasts, f, &r_set);

                        if is_reliable {
                            broadcasts.entry(broadcast.clone()).or_insert(0);

                            // Process the reliable broadcast
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
                        info!("Process {} RECEIVED RESPONSE with broadcast hash {}: step={:?}, sender={}, state={:?}, rank={}", 
                            id, response.state[0].broadcast.hash_value(), response.step, response.sender, response.state, response.rank);

                        Process::reliably_check_response(
                            id,
                            response,
                            &mut broadcasts,
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
        info!("Process {} stopping", self.id);
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    fn send_message(senders: &[Sender<Message>], message: &mut Message, byzantine: bool) {   
            if byzantine {
                let mut rng = rand::thread_rng();
                match message {
                    Message::Broadcast(broadcast) => {
                        // Randomly choose which field to change
                        match rng.gen_range(0..3) {
                            0 => broadcast.step = match broadcast.step {
                                Step::R => Step::A,
                                Step::A => Step::B,
                                Step::B => Step::R,
                            },
                            //1 => broadcast.value = rng.gen_range(0..100),
                            1 => broadcast.flag = Some(rng.gen_bool(0.5)),
                            2 => broadcast.rank = rng.gen_range(0..100),
                            _ => unreachable!(),
                        }
                    }
                    Message::Response(response) => {
                        // Randomly choose which field to change
                        match rng.gen_range(0..1) {
                            0 => response.step = match response.step {
                                Step::R => Step::A,
                                Step::A => Step::B,
                                Step::B => Step::R,
                            },
                            //1 => response.rank = rng.gen_range(0..100),
                            2 => {
                                // Randomly change one of the states
                                /*if !response.state.is_empty() {
                                    let idx = rng.gen_range(0..response.state.len());
                                    response.state[idx] = State::new(
                                        Value::RValue(RValue::new(rng.gen_range(0..100), rng.gen_range(0..100))),
                                        response.broadcast_request.clone()
                                    );
                                }*/
                            },
                            _ => unreachable!(),
                        }
                    }
                }
            }

            match message {
                Message::Broadcast(broadcast) => {
                    info!("Process {} SENDING BROADCAST with hash {}: step={:?}, rank={}, value={}, previous_step_responses={:?}", 
                        broadcast.sender, broadcast.hash_value(), broadcast.step, broadcast.rank, broadcast.value, broadcast.previous_step_responses);
                }
                Message::Response(response) => {
                    info!("Process {} SENDING RESPONSE with broadcast hash {}: step={:?}, rank={}, state={:?}", 
                        response.sender, response.state[0].broadcast.hash_value(), response.step, response.rank, response.state);
                }
            }
            
            for sender in senders {
                sender.send(message.clone()).unwrap();
            }
       
    }

    pub fn propose(&mut self, threshold: usize, value: i64, rank: Rank) -> i64 {
        loop {
            if self.stop_flag.load(Ordering::Relaxed) {
                info!("Process {} received stop signal, terminating", self.id);
                return -1;
            }
            // R Step
            let r_value = self.r_step(threshold, RValue::new(rank, value));

            // A Step
            let (flag, a_value) = self.a_step(threshold, r_value);

            // B Step
            let decision = self.b_step(threshold, r_value.rank, flag, a_value);
            
            // Return decision result
            match decision {
                Decision::Commit(val) => return val,
                Decision::Adopt(val) => self.propose(threshold, val, rank + 1)
            };

            //return r_value.value;
        }
    }

    // Line 15: procedure R-Step(v)
    fn r_step(&mut self, threshold: usize, r_value: RValue) -> RValue {
        let rank = r_value.rank;
        let value = r_value.value;
        
        info!("Process {} starting R-step with rank {} and value {}", 
              self.id, rank, value);

        // Line 16: compile certificate C (empty at rank 0) 
        // Line 90: To compile a broadcast certificate, list all 2f + 1 answers to the previous step broadcast received during the previous step.
        // Line 17: broadcast(R, i, v, C) 
        if rank > 0 {
            info!("Process {} waiting for {} B responses from rank {}", 
                  self.id, threshold, rank - 1);

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

        // Line 18/19: wait until (receive valid (Rresp, i, R, C) from 2f + 1 processes)
        let key = (Step::R, rank);

        loop {
            let responses = self.responses.read().unwrap();
            if responses.get(&key).map(|m| m.len()).unwrap_or(0) == threshold {
                // Collect the R values from the responses
                let r_values: Vec<RValue> = responses.get(&key)
                    .unwrap()
                    .values()
                    .filter_map(|response| {
                        for state in &response.state {
                            if let Value::RValue(r_value) = state.value {
                                return Some(r_value);
                            }
                        }
                        None
                    })
                    .collect();
                
                info!("Process {} collected {} R values in rank {}", 
                      self.id, r_values.len(), rank);
                
                // Return the maximum of all R values
                return *r_values.iter().max().unwrap();
            }
        }
    }

    fn r_max(r_values: Vec<RValue>, r_set: &R) -> RValue {
        // Line 20: R ← R ∪ {union of all valid Rs received in previous line}
        let mut combined_values = r_values;
        combined_values.push(r_set.read().unwrap().clone());

        // Line 22: ⟨i′,v′⟩ ← max(R) 
        let max_r_value = combined_values.iter().max().unwrap();

        // Line 23: R ← max(R) 
        {
            *r_set.write().unwrap() = *max_r_value;
        }

        // Line 24: return ⟨i′,v′⟩
        *max_r_value
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
        
        // Line 28: b ← bcast responsible for R[j]’s value
        let response_broadcast = {
            broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == max_r_value.value && rb.rank == max_r_value.rank && rb.step == broadcast.step))
                .map(|(b, _)| b.clone())
                .unwrap()
        };

        // A broadcast from pi justifies a response from pj for an R-Step if it contains the highest value encountered that appears in pj response.
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
        
        info!("Process {} starting A-step with rank {} and value {}", 
              self.id, rank, value);

        let key = (Step::R, rank);

        // Line 32: compile certificate C
        let responses = self.responses.read().unwrap().get(&key).unwrap().values().cloned().collect();
        
        let broadcast = Broadcast::new(self.id, Step::A, value, None, rank, Some(responses));

        // Line 33: broadcast(A, i, v, C)
        Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        
        info!("Process {} waiting for {} A responses", self.id, threshold);

        // Lines 34/35:  wait until receive valid (Aresp, i, A[i]) 35: from 2f + 1 processes

        let key = (Step::A, rank);

        // Collect the A values from the responses
        loop {
            let responses = self.responses.read().unwrap();
            if responses.get(&key).map(|m| m.len()).unwrap_or(0) == threshold {
                // Collect the R values from the responses
                let a_values: Vec<AValue> = responses.get(&key)
                    .unwrap()
                    .values()
                    .filter_map(|response| {
                        for state in &response.state {
                            if let Value::AValue(a_value) = state.value {
                                return Some(a_value);
                            }
                        }
                        None
                    })
                    .collect();
                
                 // Count occurrences of each value in the A-answers
                let mut value_counts: HashMap<AValue, usize> = HashMap::new();
                let mut max_value = AValue(value);

                for a_value in &a_values {
                    *value_counts.entry(*a_value).or_insert(0) += 1;
                    
                    // Track the maximum value
                    if a_value.0 > max_value.0 {
                        max_value = *a_value;
                    }
                }

                info!("Process {}: Value counts: {:?}", self.id, value_counts);
                info!("Process {}: Max value found: {}", self.id, max_value.0);

                // Line 37/38: if (S contains at least 2f+1 A-answers containing only val)
                for (val, count) in value_counts.iter() {
                    if *count >= threshold {
                        info!("Process {} returned flag {} and value {:?} in A step in rank {}", 
                            self.id, true, val.0, rank);
                        
                        // Line 39: return ⟨true, val⟩
                        return (true, val.0);
                    }
                }

                info!("Process {} returned flag {} and value {:?} in A step in rank {}", 
                    self.id, false, max_value.0, rank);

                // Line 40:  else return ⟨false, max(S)⟩
                return (false, max_value.0)
            }
        }
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
        
        // First, check if we need to update a_sets
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

        // Now handle sending responses
        let mut sent_values = HashSet::new();
        
        // Get the current a_sets state for sending responses
        let current_a_sets = {
            a_sets.read().unwrap().clone().clone()
        };

        /* A broadcast from pi justifies a response from pj for an A-Step, if it contains 
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
            //broadcast.clone()
        );
        
        // Line 48: send(Aresp, j, A[j], sig, b) to all
        Process::send_message(senders, &mut Message::Response(response), byzantine);
    }

    fn b_step(&mut self, threshold: usize, rank: Rank, flag: bool, value: i64) -> Decision {
        info!("Process {} starting B-step with rank {}, value {}, flag={}", 
              self.id, rank, value, flag);

        // Line 51: compile certificate C
        let key = (Step::A, rank);
                
        let responses = self.responses.read().unwrap().get(&key).unwrap().values().cloned().collect();
        
        let broadcast = Broadcast::new(self.id, Step::B, value, Some(flag), rank, Some(responses));
        
        // Line 52: broadcast(B, i, , v, C)
        Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        
        let key = (Step::B, rank);

        info!("Process {} waiting for {} B responses", self.id, threshold);

        // Line 53/54:  wait until receive valid (Bresp, i, B[i]) from 2f + 1 proc.
        loop {
            let responses = self.responses.read().unwrap();
            if responses.get(&key).map(|m| m.len()).unwrap_or(0) == threshold {
                // Collect the B values from the responses
                let b_values: Vec<BValue> = responses.get(&key)
                    .unwrap()
                    .values()
                    .filter_map(|response| {
                        for state in &response.state {
                            if let Value::BValue(b_value) = state.value {
                                return Some(b_value);
                            }
                        }
                        None
                    })
                    .collect();

                // Count how many true values exist
                let true_values: Vec<&BValue> = b_values.iter()
                    .filter(|&b_value| b_value.flag)
                    .collect();

            info!("Process {}: Found {} true values", self.id, true_values.len());
            
            // Line 56: if |{⟨true, val⟩ ∈ S}| ≥ 2f + 1
            if true_values.len() >= threshold {
                let value = true_values.first().unwrap().value;
                info!("Process {} returned COMMIT and value {:?} in B step in rank {}", 
                    self.id, value, rank);

                // Line 57: return ⟨commit, val⟩
                return Decision::Commit(value)
            }
            // Line 58: else if |{⟨true, val⟩ ∈ S}| ≥ 1 then
            else if !true_values.is_empty() {
                let value = true_values.first().unwrap().value;
                info!("Process {} returned ADOPT and value {:?} in B step in rank {}", 
                    self.id, value, rank);
                
                // Line 59: return ⟨adopt, val⟩
                return Decision::Adopt(value);
            }
            else {
                let max_value = b_values.iter()
                    .max_by_key(|b_value| b_value.value)
                    .map(|b_value| b_value.value)
                    .unwrap();
                
                info!("Process {} returned ADOPT and value {:?} in B step in rank {}", 
                    self.id, max_value, rank);

                // Line 60: else return ⟨adopt, max(S)⟩
                return Decision::Adopt(max_value);
            }
            }
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
        
        /*if len > j {
            let b_values = b_sets.read().unwrap().clone();
            let len = b_values.len();

            let m = match len {
                0 => 0,
                1 => b_values[0].value,
                _ => max(b_values[0].value, b_values[1].value)
            };

            if len < 2 {
                {
                    // Line 64: if |B[j]| < 2 then add ⟨bool, v⟩ to B[j]
                    b_sets.write().unwrap().push(b_value);
                }
            }
            else {
                let contains_flag_value = {
                    b_sets.read().unwrap().iter().any(|value| value == &b_value)
                };
                
                // Line 65: else if(flag ∧ ⟨flag, v⟩ ∈/ B[j] ∨ ¬flag ∧ v > m) then
                if (flag && !contains_flag_value) || (!flag && value > m) {
                    // Line 66: B[j][0] ← ⟨flag, v⟩
                    {
                        b_sets.write().unwrap()[0] = b_value;
                    }
                }
            }
        }
        else {
            {
                b_sets.write().unwrap().push(b_value);
            }
        }*/

        /* For a broadcast from pi to justify a response from pj for a B-Step, it must ensures the following: 
        if the response contains only true, then the broadcast should contain true; 
        if the response contains at least one true and false pair, then the broadcast should contain the true pair, and any of the false pairs; 
        if the response contains only false pairs, then the broadcast should contain the pair among them with the highest value. */
        
        // Get true and false pairs
        let b_values = {
            b_sets.read().unwrap().clone()
        };
        
        let true_pairs: Vec<&BValue> = b_values.iter()
            .filter(|b_state| b_state.flag)
            .collect();
        
        let false_pairs: Vec<&BValue> = b_values.iter()
            .filter(|b_state| !b_state.flag)
            .collect();

            let response = Response::new(
                id, 
                Step::B,
                broadcast.rank, 
                vec![State::new(Value::BValue(b_value), broadcast.clone())], 
                //broadcast.clone()
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        
        // Case 1: Only true pairs
        /*if !true_pairs.is_empty() && false_pairs.is_empty() {
            let b_value = *true_pairs[0];

            let response_broadcast = broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == b_value.value && rb.rank == broadcast.rank && rb.step == broadcast.step))
                .unwrap()
                .0
                .clone();

            // Send one response with one of the true pairs
            let response = Response::new(
                id, 
                Step::B,
                broadcast.rank, 
                vec![State::new(Value::BValue(b_value), response_broadcast)], 
                //broadcast.clone()
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        }
        // Case 2: At least one true and one false pair
        else if !true_pairs.is_empty() && !false_pairs.is_empty() {
            let mut b_state = Vec::new();

            let b_value_true = *true_pairs[0];

            let response_broadcast_true = broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == b_value_true.value && rb.rank == broadcast.rank && rb.step == broadcast.step))
                .unwrap()
                .0
                .clone();

            b_state.push(State::new(Value::BValue(b_value_true), response_broadcast_true));
        
            let b_value_false = *false_pairs[0];

            let response_broadcast_false = broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == b_value_false.value && rb.rank == broadcast.rank && rb.step == broadcast.step))
                .unwrap()
                .0
                .clone();

            b_state.push(State::new(Value::BValue(b_value_false), response_broadcast_false));

            let response = Response::new(
                id, 
                Step::B,
                broadcast.rank, 
                b_state, 
                //broadcast.clone()
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        }
        // Case 3: Only false pairs
        else if true_pairs.is_empty() && !false_pairs.is_empty() {                                        
            // Send response with the false pair having the highest value
            let highest_false = false_pairs.iter()
                .max_by_key(|b_state| b_state.value)
                .unwrap();

            let response_broadcast = broadcasts
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == highest_false.value && rb.rank == broadcast.rank && rb.step == broadcast.step))
                .unwrap()
                .0
                .clone();
            
            let response = Response::new(
                id, 
                Step::B,
                broadcast.rank, 
                vec![State::new(Value::BValue(**highest_false), response_broadcast)], 
                //broadcast.clone()
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        }*/
    }

    /*fn validate_response(response: &Response) -> bool {
        for state in &response.state {
            let broadcast = &state.broadcast;
            
            match response.step {
                Step::R => {
                    // For R step at rank r, must have B step at rank r-1
                    if broadcast.step != Step::R || broadcast.rank != response.rank {
                        info!("Invalid R response: expected R step at rank {}, got {:?} at rank {}", 
                              response.rank, broadcast.step, broadcast.rank);
                        return false;
                    }
                }
                Step::A => {
                    // For A step at rank r, must have R step at rank r
                    if broadcast.step != Step::A || broadcast.rank != response.rank {
                        info!("Invalid A response: expected A step at rank {}, got {:?} at rank {}", 
                              response.rank, broadcast.step, broadcast.rank);
                        return false;
                    }
                }
                Step::B => {
                    // For B step at rank r, must have A step at rank r
                    if broadcast.step != Step::B || broadcast.rank != response.rank {
                        info!("Invalid B response: expected B step at rank {}, got {:?} at rank {}", 
                              response.rank, broadcast.step, broadcast.rank);
                        return false;
                    }
                }
            }
        }
        true
    }*/

    fn reliably_check_response(
        id: Id,
        response: Response,
        broadcasts: &mut Broadcasts,
        responses: &Responses,
        pending_responses: &mut PendingResponses,
        threshold: usize
    ) {
        info!("Process {} handling response from {}: step={:?}, rank={}", 
              id, response.sender, response.step, response.rank);
              
        // Extract broadcast hashes from response states
        let broadcast_hashes: BTreeSet<BroadcastHash> = response.state.iter()
            .map(|r| r.broadcast.hash_value())
            .collect();

        // Add to pending responses
        if !pending_responses.contains_key(&broadcast_hashes) {
            info!("Process {}: Creating new pending response entry", id);
            let mut responses_set = HashSet::new();
            responses_set.insert(response.clone());
            pending_responses.insert(broadcast_hashes.clone(), responses_set);
        } else {
            info!("Process {}: Adding to existing pending responses", id);
            pending_responses.get_mut(&broadcast_hashes).unwrap().insert(response.clone());
        }

        // Check if we've reached the threshold
        if let Some(received_responses) = pending_responses.get(&broadcast_hashes) {
            info!("Process {}: Current pending responses count: {}/{}", 
                id, received_responses.len(), threshold);
                
            if received_responses.len() >= threshold {
                info!("Process {}: Threshold reached for responses ({}), storing in responses map", 
                    id, threshold);
                    
                // Store each response in the responses map
                for resp in received_responses {
                    let key = (resp.step.clone(), resp.rank);
                    let mut responses_map = responses.write().unwrap();
                    responses_map.entry(key)
                        .or_insert_with(HashMap::new)
                        .insert(resp.sender, resp.clone());
                }
            }
        }
    }

    /*fn validate_a_step(
        broadcast: &Broadcast,
        responses: &Option<Vec<Response>>,
        threshold: usize
    ) -> bool {
        let mut a_values: Vec<AValue> = Vec::new();
        let mut unique_processes = HashSet::new();

        // Line 34/35: wait until receive valid (Aresp, i, A[i]) from 2f + 1 processes
        while a_values.len() < threshold {
            for a_response in responses.as_ref().unwrap().iter() {
                if unique_processes.insert(a_response.sender) {
                    for a_state in &a_response.state {
                        if let Value::AValue(a_value) = a_state.value {
                            a_values.push(a_value);
                        }
                    }
                }
            }
        }

        // Count occurrences of each value in the A-answers
        let mut value_counts: HashMap<AValue, usize> = HashMap::new();
        let mut max_value = AValue(broadcast.value);

        for a_value in &a_values {
            *value_counts.entry(*a_value).or_insert(0) += 1;
            
            // Track the maximum value
            if a_value.0 > max_value.0 {
                max_value = *a_value;
            }
        }

        // Line 37/38: if (S contains at least 2f+1 A-answers containing only val)
        for (val, count) in value_counts.iter() {
            if *count >= threshold {
                // Line 39: return true if value matches
                return val.0 == broadcast.value;
            }
        }

        // Line 40: else return true if max value matches
        max_value.0 == broadcast.value
    }

    fn validate_b_step(
        broadcast: &Broadcast,
        responses: &Option<Vec<Response>>,
        threshold: usize
    ) -> bool {
        let mut b_values: Vec<BValue> = Vec::new();
        let mut unique_processes = HashSet::new();

        // Line 53/54: wait until receive valid (Bresp, i, B[i]) from 2f + 1 proc.
        while b_values.len() < threshold {
            for b_response in responses.as_ref().unwrap().iter() {
                if unique_processes.insert(b_response.sender) {
                    for b_state in &b_response.state {
                        if let Value::BValue(b_value) = b_state.value {
                            b_values.push(b_value);
                        }
                    }
                }
            }
        }

        // Count how many true values exist
        let true_values: Vec<&BValue> = b_values.iter()
            .filter(|b_value| b_value.flag)
            .collect();

        // Line 56: if |{⟨true, val⟩ ∈ S}| ≥ 2f + 1
        if true_values.len() >= threshold {
            let value = true_values.first().unwrap().value;
            value == broadcast.value
        }
        // Line 58: else if |{⟨true, val⟩ ∈ S}| ≥ 1 then
        else if !true_values.is_empty() {
            let value = true_values.first().unwrap().value;
            value == broadcast.value
        }
        else {
            let max_value = b_values.iter()
                .max_by_key(|b_value| b_value.value)
                .map(|b_value| b_value.value)
                .unwrap();
            
            max_value == broadcast.value
        }
    }*/

    fn reliably_check_broadcast(
        broadcast: &Broadcast,
        broadcasts: &Broadcasts,
        f: usize,
        r_set: &R,
    ) -> bool {
        if broadcast.step == Step::R && broadcast.rank == 0 {
            return true;
        }

        if broadcast.previous_step_responses.is_none() {
            return false;
        }

        let threshold = 2 * f + 1;
        let responses = broadcast.previous_step_responses.clone();

        // Line 74/75: if |{bcast-answers ∈ C}| > f then return true
        // There are at least f responses in the certificate that have been reliably verified
        if *broadcasts.get(&broadcast).unwrap_or(&0) as usize > f {
            return true;
        }

        // Line 76: check that |C| ≥ 2f + 1 messages 
        //if count < threshold {
            //return false;
        //}
        
        // Missing
        // Line 77: check signatures of those messages 
        // Line 78: check if |{bcast-answers }| > f

        match broadcast.step {
            Step::R => {
                if broadcast.rank == 0 {
                    true
                } else {
                    true
                    //Process::validate_b_step(broadcast, &responses, threshold)
                }
            }
            Step::A => {
                true
            }
            Step::B => {
                if broadcast.flag.is_none() {
                    return false;
                }

                true
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
            
            //let mut process4 = Process::new(3, f, vec![sender1.clone(), sender2.clone(), sender3.clone(), sender4.clone()], receiver4, false);
            //let mut process4_clone = process4.clone();

            let p1 = thread::spawn(move || {
                process1.propose(threshold, instance + 0, 0)
            });
            
            let p2 = thread::spawn(move || {
                process2.propose(threshold, instance + 1, 0)
            });
            
            let p3 = thread::spawn(move || {
                process3.propose(threshold, instance + 2, 0)
            });

            //let p4 = thread::spawn(move || {
                //process4.propose(threshold, instance + 3, 0)
            //});
            
            let p1_value = p1.join().unwrap();
            let p2_value = p2.join().unwrap();
            let p3_value = p3.join().unwrap();
            //let p4_value = p4.join().unwrap();

            process1_clone.stop();
            process2_clone.stop();
            process3_clone.stop();
            //process4_clone.stop();

            //assert_eq!(p1_value, p2_value);
            //assert_eq!(p1_value, p3_value);
        }
    }
}