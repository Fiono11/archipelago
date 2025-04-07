use std::{cmp::max, collections::{HashMap, HashSet}, sync::{mpsc::{Receiver, Sender}, Arc, RwLock}, thread};
use crate::{AValue, BValue, Broadcast, Decision, Id, Message, RValue, Rank, Response, Responses, State, Step, Value};
use log::info;
use rand::{self, Rng};

#[derive(Debug, Clone)]
pub struct Process {
    id: Id,
    r_set: Arc<RwLock<Vec<RValue>>>,
    a_sets: Arc<RwLock<Vec<Vec<AValue>>>>,
    b_sets: Arc<RwLock<Vec<Vec<BValue>>>>,
    responses: Arc<RwLock<HashMap<(Step, Rank), HashMap<Id, Response>>>>,
    senders: Vec<Sender<Message>>,
    stop_flag: Arc<RwLock<bool>>,
    byzantine: bool,
}

impl Process {
    pub fn new(id: Id, f: usize, senders: Vec<Sender<Message>>, receiver: Receiver<Message>, byzantine: bool) -> Self {   
        let broadcasts = Arc::new(RwLock::new(HashMap::new()));
        let responses = Arc::new(RwLock::new(HashMap::new()));
        let responses_clone = responses.clone();
        let senders_clone = senders.clone();
        let r_set = Arc::new(RwLock::new(Vec::new()));
        let a_sets = Arc::new(RwLock::new(Vec::new()));
        let b_sets = Arc::new(RwLock::new(Vec::new()));
        let pending_responses = Arc::new(RwLock::new(HashMap::new()));
        let r_set_clone: Arc<RwLock<Vec<RValue>>> = Arc::clone(&r_set);
        let a_sets_clone = Arc::clone(&a_sets);
        let b_sets_clone = Arc::clone(&b_sets);
        let stop_flag = Arc::new(RwLock::new(false));
        let stop_flag_clone = stop_flag.clone();

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
                broadcasts,
                responses_clone,
                senders_clone,
                r_set_clone,
                a_sets_clone,
                b_sets_clone,
                pending_responses,
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
        broadcasts: Arc<RwLock<HashMap<Broadcast, Responses>>>,
        responses: Arc<RwLock<HashMap<(Step, Rank), HashMap<Id, Response>>>>,
        senders: Vec<Sender<Message>>,
        r_set: Arc<RwLock<Vec<RValue>>>,
        a_sets: Arc<RwLock<Vec<Vec<AValue>>>>,
        b_sets: Arc<RwLock<Vec<Vec<BValue>>>>,
        pending_responses: Arc<RwLock<HashMap<Response, Vec<State>>>>,
        stop_flag: Arc<RwLock<bool>>,
        receiver: Receiver<Message>,
        byzantine: bool
    ) {
        loop {
            // Check if we should stop
            if *stop_flag.read().unwrap() {
                info!("Process {} received stop signal, terminating", id);
                break;
            }

            if let Ok(msg) = receiver.recv() {
                match msg {
                    Message::Broadcast(broadcast) => {                        
                        info!("Process {} RECEIVED BROADCAST: step={:?}, sender={}, rank={}, value={}, previous_step_responses={:?}", 
                            id, broadcast.step, broadcast.sender, broadcast.rank, broadcast.value, broadcast.previous_step_responses);

                        {
                            let mut broadcasts_write = broadcasts.write().unwrap();
                            broadcasts_write.entry(broadcast.clone()).or_insert(0);
                        }

                        // Lines 26, 42, 62
                        let is_reliable = Process::reliably_check_broadcast(&broadcast, f, &r_set);

                        if is_reliable {
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
                        info!("Process {} RECEIVED RESPONSE: step={:?}, sender={}, state={:?}, rank={}, broadcast_request={:?}", 
                            id, response.step, response.sender, response.state, response.rank, response.broadcast_request);

                        Process::handle_response(
                            response,
                            &broadcasts,
                            &responses,
                            &pending_responses,
                            2 * f + 1
                        );
                    }
                }
            }
        }
    }

    pub fn stop(&self) {
        info!("Process {} stopping", self.id);
        *self.stop_flag.write().unwrap() = true;
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
                                if !response.state.is_empty() {
                                    let idx = rng.gen_range(0..response.state.len());
                                    response.state[idx] = State::new(
                                        Value::RValue(RValue::new(rng.gen_range(0..100), rng.gen_range(0..100))),
                                        response.broadcast_request.clone()
                                    );
                                }
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
                    info!("Process {} SENDING RESPONSE: step={:?}, rank={}, state={:?}, broadcast_request={:?}", 
                        response.sender, response.step, response.rank, response.state, response.broadcast_request);
                }
            }
            
            for sender in senders {
                sender.send(message.clone()).unwrap();
            }
       
    }

    pub fn propose(&mut self, threshold: usize, value: i64, rank: Rank) -> i64 {
        loop {
            if *self.stop_flag.read().unwrap() {
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
    }
    }

    // Line 15: procedure R-Step(v)
    fn r_step(&mut self, threshold: usize, r_value: RValue) -> RValue {
        let rank = r_value.rank;
        let value = r_value.value;
        
        info!("Process {} starting R-step with rank {} and value {}", 
              self.id, rank, value);

        let r_set_values: Vec<Value> = self.r_set.read().unwrap().iter()
            .map(|r| Value::RValue(r.clone()))
            .collect();

        // Line 16: compile certificate C (empty at rank 0) 
        // Line 90: To compile a broadcast certificate, list all 2f + 1 answers to the previous step broadcast received during the previous step.
        // Line 17: broadcast(R, i, v, C) 
        if rank > 0 {
            info!("Process {} waiting for {} B responses from rank {}", 
                  self.id, threshold, rank - 1);

            let mut responses: Vec<Response> = Vec::new();
            let mut unique_processes = HashSet::new();

            while responses.len() < threshold {
                let key = (Step::B, rank - 1);
                let b_responses = {
                    let responses_read = self.responses.read().unwrap();
                    if let Some(step_responses) = responses_read.get(&key) {
                        step_responses.values().cloned().collect::<Vec<Response>>()
                    } else {
                        Vec::new()
                    }
                };

                for b_response in b_responses {
                    if unique_processes.insert(b_response.sender) {
                        responses.push(b_response);
                    }
                }
            }
                    
            let broadcast = Broadcast::new(self.id, vec![r_set_values], Step::R, value, None, rank, Some(responses));
            
            Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        }
        else {
            let broadcast = Broadcast::new(self.id, vec![r_set_values], Step::R, r_value.value, None, r_value.rank, None);
                
            Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        }

        let mut r_values = Vec::new();
        let mut unique_processes = HashSet::new();

        // Line 18/19: wait until (receive valid (Rresp, i, R, C) from 2f + 1 processes)
        while r_values.len() < threshold {
            let key = (Step::R, rank);
            let r_responses = {
                let responses_read = self.responses.read().unwrap();
                if let Some(step_responses) = responses_read.get(&key) {
                    step_responses.values().cloned().collect::<Vec<Response>>()
                } else {
                    Vec::new()
                }
            };

            for r_response in &r_responses {
                if unique_processes.insert(r_response.sender) {
                    if let Value::RValue(r_value) = r_response.state[0].value {
                        r_values.push(r_value);
                    }
                }
            }
        } 

        Process::r_max(r_values, &self.r_set)
    }

    fn r_max(r_values: Vec<RValue>, r_set: &Arc<RwLock<Vec<RValue>>>) -> RValue {
        // Line 20: R ← R ∪ {union of all valid Rs received in previous line}
        let mut combined_values = r_values;
        combined_values.extend(r_set.read().unwrap().clone());

        // Line 22: ⟨i′,v′⟩ ← max(R) 
        let max_r_value = combined_values.iter().max().unwrap();

        // Line 23: R ← max(R) 
        {
            r_set.write().unwrap().push(*max_r_value);
        }

        // Line 24: return ⟨i′,v′⟩
        *max_r_value
    }

    // Line 25: Upon delivering (R, j, v, C) from p
    fn answer_r_broadcast(
        id: Id,
        broadcast: &Broadcast,
        senders: &[Sender<Message>],
        r_set: &Arc<RwLock<Vec<RValue>>>,
        broadcasts: &Arc<RwLock<HashMap<Broadcast, Responses>>>,
        byzantine: bool
    ) {
        let broadcast_r_value = RValue::new(broadcast.rank, broadcast.value);

        // Line 27: R ← max(⟨j, v⟩, R)
        let max_r_value = max(broadcast_r_value, *r_set.read().unwrap().iter().max().unwrap_or(&broadcast_r_value));
        {
            let mut r_set_write = r_set.write().unwrap();
            r_set_write.clear();
            r_set_write.push(max_r_value);
        }

        let broadcasts = broadcasts.read().unwrap().clone();
        
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
            broadcast.clone()
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

        let mut responses = Vec::new();
        let mut unique_processes = HashSet::new();

        // Line 32: compile certificate C
        while responses.len() < threshold {
            let key = (Step::R, rank);
            let r_responses = {
                let responses_read = self.responses.read().unwrap();
                if let Some(step_responses) = responses_read.get(&key) {
                    step_responses.values().cloned().collect::<Vec<Response>>()
                } else {
                    Vec::new()
                }
            };

            for r_response in r_responses {
                if unique_processes.insert(r_response.sender) {
                    responses.push(r_response);
                }
            }
        }

        let a_sets_values: Vec<Vec<Value>> = self.a_sets.read().unwrap().iter()
            .map(|set| set.iter().map(|a| Value::AValue(a.clone())).collect())
            .collect();
        
        let broadcast = Broadcast::new(self.id, a_sets_values, Step::A, value, None, rank, Some(responses.clone()));

        // Line 33: broadcast(A, i, v, C)
        Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        
        let mut a_values: Vec<AValue> = Vec::new();
        let mut unique_processes = HashSet::new();

        info!("Process {} waiting for {} A responses", self.id, threshold);

        // Lines 34/35:  wait until receive valid (Aresp, i, A[i]) 35: from 2f + 1 processes
        while a_values.len() < threshold {
            let key = (Step::A, rank);
            let a_responses = {
                let responses_read = self.responses.read().unwrap();
                if let Some(step_responses) = responses_read.get(&key) {
                    step_responses.values().cloned().collect::<Vec<Response>>()
                } else {
                    Vec::new()
                }
            };

            // Line 36: S ← union of all A[i]s received
            for a_response in &a_responses {
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
        let mut max_value = AValue(value);

        for a_value in &a_values {
            *value_counts.entry(a_value.clone()).or_insert(0) += 1;
            
            // Track the maximum value
            if a_value.0 > max_value.0 {
                max_value = a_value.clone();
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
        (false, max_value.0)
    }

    fn answer_a_broadcast(
        id: Id,
        broadcast: &Broadcast,
        senders: &[Sender<Message>],
        a_sets: &Arc<RwLock<Vec<Vec<AValue>>>>,
        broadcasts: &Arc<RwLock<HashMap<Broadcast, Responses>>>,
        byzantine: bool
    ) {
        let j = broadcast.rank as usize;
        let broadcast_value = AValue(broadcast.value);
        
        // First, check if we need to update a_sets
        {
            let mut a_sets_write = a_sets.write().unwrap();
            if a_sets_write.len() > j {
                let current_set = &mut a_sets_write[j];
                    // Line 43: if v /∈ A[j] and |A[j]| < 2
                    if !current_set.contains(&broadcast_value) && current_set.len() < 2 {
                        // Line 44: add v to A[j]
                        current_set.push(broadcast_value);
                    // Line 45: v > max(A[j])
                    } else if broadcast_value > *current_set.iter().max().unwrap() {
                        let min = current_set.iter().min().unwrap();
                        if let Some(index) = current_set.iter().position(|&x| x == *min) {
                            // Line 46: min(A[j]) ← v
                            current_set[index] = broadcast_value;
                        }
                    }
            }
            else {
                a_sets_write.push(vec![broadcast_value]);
            }
        }

        // Now handle sending responses
        let mut sent_values = HashSet::new();
        
        // Get the current a_sets state for sending responses
        let current_a_sets = {
            a_sets.read().unwrap().clone()[j].clone()
        };

        /* A broadcast from pi justifies a response from pj for an A-Step, if it contains 
        the highest value v and, if possible, 
        any value from the response different from v. */

        let mut a_states = Vec::new();
        let broadcasts = broadcasts.read().unwrap().clone();
        
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
            broadcast.clone()
        );
        
        // Line 48: send(Aresp, j, A[j], sig, b) to all
        Process::send_message(senders, &mut Message::Response(response), byzantine);
    }

    fn b_step(&mut self, threshold: usize, rank: Rank, flag: bool, value: i64) -> Decision {
        info!("Process {} starting B-step with rank {}, value {}, flag={}", 
              self.id, rank, value, flag);

        let mut responses = Vec::new();
        let mut unique_processes = HashSet::new();

        // Line 51: compile certificate C
        while responses.len() < threshold {
            let key = (Step::A, rank);
            let a_responses = {
                let responses_read = self.responses.read().unwrap();
                if let Some(step_responses) = responses_read.get(&key) {
                    step_responses.values().cloned().collect::<Vec<Response>>()
                } else {
                    Vec::new()
                }
            };

            for a_response in a_responses {
                if unique_processes.insert(a_response.sender) {
                    responses.push(a_response);
                }
            }
        }

        let b_sets_values: Vec<Vec<Value>> = self.b_sets.read().unwrap().iter()
            .map(|set| set.iter().map(|a| Value::BValue(a.clone())).collect())
            .collect();
        
        let broadcast = Broadcast::new(self.id, b_sets_values, Step::B, value, Some(flag), rank, Some(responses));
        
        // Line 52: broadcast(B, i, , v, C)
        Process::send_message(&self.senders, &mut Message::Broadcast(broadcast), self.byzantine);
        
        let mut b_values = Vec::new();
        let mut unique_processes = HashSet::new();

        info!("Process {} waiting for {} B responses", self.id, threshold);

        // Line 53/54:  wait until receive valid (Bresp, i, B[i]) from 2f + 1 proc.
        while b_values.len() < threshold {
            let key = (Step::B, rank);
            let b_responses = {
                let responses_read = self.responses.read().unwrap();
                if let Some(step_responses) = responses_read.get(&key) {
                    step_responses.values().cloned().collect::<Vec<Response>>()
                } else {
                    Vec::new()
                }
            };

            for b_response in b_responses {
                if unique_processes.insert(b_response.sender) {
                    for b_state in &b_response.state {
                        if let Value::BValue(b_value) = b_state.value {
                            // Line 55: S ← array with all B[i]s received
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

        info!("Process {}: Found {} true values", self.id, true_values.len());
        
        // Line 56: if |{⟨true, val⟩ ∈ S}| ≥ 2f + 1
        if true_values.len() >= threshold {
            let value = true_values.first().unwrap().value;
            info!("Process {} returned COMMIT and value {:?} in B step in rank {}", 
                  self.id, value, rank);

            // Line 57: return ⟨commit, val⟩
            return Decision::Commit(value);
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

    fn answer_b_broadcast(
        id: Id,
        broadcast: &Broadcast,
        senders: &[Sender<Message>],
        b_sets: &Arc<RwLock<Vec<Vec<BValue>>>>,
        broadcasts: &Arc<RwLock<HashMap<Broadcast, Responses>>>,
        byzantine: bool
    ) {
        let len = {
            b_sets.read().unwrap().len()
        };
        let j = broadcast.rank as usize;

        let value = broadcast.value;
        let flag = broadcast.flag.unwrap();
        let b_value = BValue::new(value, flag);
        
        if len > j {
            let b_values = b_sets.read().unwrap().clone()[j].clone();
            let len = b_values.len();

            let m = if len > 1 {
                // Line 63: m ← max(B[j][0].v, B[j][1].v)
                max(b_values[0].value, b_values[1].value)
            }
            else if len == 1 {
                b_values[0].value
            }
            else {
                0
            };

            if len < 2 {
                {
                    // Line 64: if |B[j]| < 2 then add ⟨bool, v⟩ to B[j]
                    b_sets.write().unwrap()[j].push(b_value);
                }
            }
            else {
                let contains_flag_value = {
                    b_sets.read().unwrap()[j].iter().any(|value| value == &b_value)
                };
                
                // Line 65: else if(flag ∧ ⟨flag, v⟩ ∈/ B[j] ∨ ¬flag ∧ v > m) then
                if (flag && !contains_flag_value) || (!flag && value > m) {
                    // Line 66: B[j][0] ← ⟨flag, v⟩
                    {
                        b_sets.write().unwrap()[j][0] = b_value;
                    }
                }
            }
        }
        else {
            {
                b_sets.write().unwrap().push(vec![b_value]);
            }
        }

        /* For a broadcast from pi to justify a response from pj for a B-Step, it must ensures the following: 
        if the response contains only true, then the broadcast should contain true; 
        if the response contains at least one true and false pair, then the broadcast should contain the true pair, and any of the false pairs; 
        if the response contains only false pairs, then the broadcast should contain the pair among them with the highest value. */
        
        // Get true and false pairs
        let b_values = {
            b_sets.read().unwrap()[j].clone()
        };
        
        let true_pairs: Vec<&BValue> = b_values.iter()
            .filter(|b_state| b_state.flag)
            .collect();
        
        let false_pairs: Vec<&BValue> = b_values.iter()
            .filter(|b_state| !b_state.flag)
            .collect();
        
        // Case 1: Only true pairs
        if !true_pairs.is_empty() && false_pairs.is_empty() {
            let b_value = true_pairs[0].clone();

            let response_broadcast = broadcasts.read().unwrap()
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
                broadcast.clone()
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        }
        // Case 2: At least one true and one false pair
        else if !true_pairs.is_empty() && !false_pairs.is_empty() {
            let mut b_state = Vec::new();

            let b_value_true = true_pairs[0].clone();

            let response_broadcast_true = broadcasts.read().unwrap()
                .iter()
                .find(|(b, _)| matches!(b, rb if rb.value == b_value_true.value && rb.rank == broadcast.rank && rb.step == broadcast.step))
                .unwrap()
                .0
                .clone();

            b_state.push(State::new(Value::BValue(b_value_true), response_broadcast_true));
        
            let b_value_false = false_pairs[0].clone();

            let response_broadcast_false = broadcasts.read().unwrap()
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
                broadcast.clone()
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        }
        // Case 3: Only false pairs
        else if true_pairs.is_empty() && !false_pairs.is_empty() {                                        
            // Send response with the false pair having the highest value
            let highest_false = false_pairs.iter()
                .max_by_key(|b_state| b_state.value)
                .unwrap();

            let response_broadcast = broadcasts.read().unwrap()
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
                broadcast.clone()
            );

            Process::send_message(senders, &mut Message::Response(response), byzantine);
        }
    }

    fn validate_response(response: &Response) -> bool {
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
    }

    fn handle_response(
        response: Response,
        broadcasts: &Arc<RwLock<HashMap<Broadcast, Responses>>>,
        responses: &Arc<RwLock<HashMap<(Step, Rank), HashMap<Id, Response>>>>,
        pending_responses: &Arc<RwLock<HashMap<Response, Vec<State>>>>,
        threshold: usize
    ) {
        info!("Handling response: step={:?}, sender={}, rank={}", 
              response.step, response.sender, response.rank);

        // Validate the response before processing
        if !Process::validate_response(&response) {
            info!("Invalid response received from process {} at step {:?} and rank {}", 
                  response.sender, response.step, response.rank);
            return;
        }

        {
            pending_responses.write().unwrap().insert(response.clone(), response.state.clone());
        }

        // Update broadcast counts and check if we have enough responses
        {
            let mut broadcasts = broadcasts.write().unwrap();

            // First update all counts
            let count = broadcasts.entry(response.broadcast_request)
                .or_insert(0);
            *count += 1;
        }

        {
            let mut all_broadcasts_reached_threshold = true;
            let broadcasts = broadcasts.read().unwrap();
            let pending_responses = pending_responses.read().unwrap();

            for (response, states) in pending_responses.iter() {
                // Then check if all broadcasts have reached threshold
                for state in states {
                    if let Some(count) = broadcasts.get(&state.broadcast) {
                        if *count < threshold as i64 {
                            info!("Broadcast {:?} in response {:?} has not reached threshold (>= {})", state.broadcast, response, threshold);

                            all_broadcasts_reached_threshold = false;
                            break;
                        }
                    } else {
                        info!("Broadcast {:?} in response {:?} not found in broadcasts map", state.broadcast, response);
                        all_broadcasts_reached_threshold = false;
                        break;
                    }
                }

                // Only add response if all its broadcasts have reached threshold
                if all_broadcasts_reached_threshold {
                    // Add to responses collection, limiting by rank and step
                    let key = (response.step.clone(), response.rank);
                    let mut responses_write = responses.write().unwrap();
                    
                    // Initialize the inner HashMap if not present
                    let step_responses = responses_write.entry(key).or_insert_with(HashMap::new);
                    // Only insert if we haven't reached threshold responses for this step/rank
                    if step_responses.len() < threshold {
                        step_responses.insert(response.sender, response.clone());
                    }
                }
            }
        }
    }

    fn validate_a_step(
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
            *value_counts.entry(a_value.clone()).or_insert(0) += 1;
            
            // Track the maximum value
            if a_value.0 > max_value.0 {
                max_value = a_value.clone();
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
            return value == broadcast.value;
        }
        // Line 58: else if |{⟨true, val⟩ ∈ S}| ≥ 1 then
        else if !true_values.is_empty() {
            let value = true_values.first().unwrap().value;
            return value == broadcast.value;
        }
        else {
            let max_value = b_values.iter()
                .max_by_key(|b_value| b_value.value)
                .map(|b_value| b_value.value)
                .unwrap();
            
            return max_value == broadcast.value;
        }
    }

    fn reliably_check_broadcast(
        broadcast: &Broadcast,
        f: usize,
        r_set: &Arc<RwLock<Vec<RValue>>>,
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
        let mut matching_responses = 0;
        let mut count = 0;

        if let Some(certificate_responses) = &responses {
            for cert_response in certificate_responses {
                count += 1;
                if let Some(process_responses) = &responses {
                    if process_responses.iter().any(|r| r == cert_response) {
                        matching_responses += 1;
                    }
                }
            }
        }

        if matching_responses > f {
            return true;
        }

        // Line 76: check that |C| ≥ 2f + 1 messages 
        if count < threshold {
            return false;
        }
        
        // Missing
        // Line 77: check signatures of those messages 
        // Line 78: check if |{bcast-answers }| > f

        match broadcast.step {
            Step::R => {
                if broadcast.rank == 0 {
                    true
                } else {
                    Process::validate_b_step(broadcast, &responses, threshold)
                }
            }
            Step::A => {
                let mut r_values = Vec::new();
                let mut unique_processes = HashSet::new();

                // Line 18/19: wait until (receive valid (Rresp, i, R, C) from 2f + 1 processes)
                while r_values.len() < threshold {
                    for r_response in responses.as_ref().unwrap().iter() {
                        if unique_processes.insert(r_response.sender) {
                            if let Value::RValue(r_value) = r_response.state[0].value {
                                r_values.push(r_value);
                            }
                        }
                    }
                } 

                Process::r_max(r_values, r_set) == RValue::new(broadcast.rank, broadcast.value)
            }
            Step::B => {
                if broadcast.flag.is_none() {
                    return false;
                }

                Process::validate_a_step(broadcast, &responses, threshold)
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
            let process1_clone = process1.clone();
            
            let mut process2 = Process::new(1, f, vec![sender1.clone(), sender2.clone(), sender3.clone(), sender4.clone()], receiver2, false);
            let process2_clone = process2.clone();
            
            let mut process3 = Process::new(2, f, vec![sender1.clone(), sender2.clone(), sender3.clone(), sender4.clone()], receiver3, false);
            let process3_clone = process3.clone();
            
            let process4 = Process::new(3, f, vec![sender1.clone(), sender2.clone(), sender3.clone(), sender4.clone()], receiver4, true);
            let process4_clone = process4.clone();

            let p1 = thread::spawn(move || {
                process1.propose(threshold, instance + 0, 0)
            });
            
            let p2 = thread::spawn(move || {
                process2.propose(threshold, instance + 1, 0)
            });
            
            let p3 = thread::spawn(move || {
                process3.propose(threshold, instance + 2, 0)
            });
            
            let p1_value = p1.join().unwrap();
            let p2_value = p2.join().unwrap();
            let p3_value = p3.join().unwrap();

            process1_clone.stop();
            process2_clone.stop();
            process3_clone.stop();
            process4_clone.stop();

            assert_eq!(p1_value, p2_value);
            assert_eq!(p1_value, p3_value);
        }
    }
}