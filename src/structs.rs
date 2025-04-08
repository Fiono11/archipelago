use std::{cmp::Ordering, hash::{DefaultHasher, Hash, Hasher}};

pub type Id = i64;
pub type Rank = i64;
pub type BroadcastHash = u64;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum Message {
    Broadcast(Broadcast),
    Response(Response),
}

// A process only sends one broadcast per step and rank
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Broadcast {
    pub sender: Id,
    pub register: Vec<Vec<Value>>,
    pub step: Step,
    pub value: i64,
    pub flag: Option<bool>,
    pub rank: Rank,
    pub previous_step_responses: Option<Vec<Response>>
}

impl Hash for Broadcast {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.step.hash(state);
        self.sender.hash(state);
        self.rank.hash(state);
    }
}

impl Broadcast {
    pub fn new(sender: Id, register: Vec<Vec<Value>>, step: Step, value: i64, flag: Option<bool>, rank: Rank, previous_step_responses: Option<Vec<Response>>) -> Broadcast {
        Broadcast { sender, register, step, value, flag, rank, previous_step_responses }
    }

    pub fn hash_value(&self) -> BroadcastHash {
        let mut state = DefaultHasher::new();
        self.step.hash(&mut state);
        self.value.hash(&mut state);
        self.flag.hash(&mut state);
        self.rank.hash(&mut state);
        state.finish()
    }
}

// Page 25 of the technical report: "Since processes can only ever send one B-answer to each process..."
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Response {
    pub sender: Id,
    pub step: Step, 
    pub rank: Rank,
    pub state: Vec<State>,
    pub broadcast_request: Broadcast,
}

impl Response {
    pub fn new(sender: Id, step: Step, rank: Rank, state: Vec<State>, broadcast_request: Broadcast) -> Self {
        Self { sender, step, rank, state, broadcast_request }
    }
}

impl Hash for Response {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.step.hash(state);
        self.sender.hash(state);
        self.rank.hash(state);
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum Step {
    R, 
    A, 
    B
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum Value {
    RValue(RValue), 
    AValue(AValue), 
    BValue(BValue),
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct State {
    pub value: Value,
    pub broadcast: Broadcast,
}

impl State {
    pub fn new(value: Value, broadcast: Broadcast) -> Self {
        Self { value, broadcast }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Default, Copy, PartialOrd, Ord)]
pub struct AValue(pub i64);

#[derive(Debug, Clone, Hash, Eq, PartialEq, Default, Copy)]
pub struct BValue {
    pub value: i64, 
    pub flag: bool,
}

impl BValue {
    pub fn new(value: i64, flag: bool) -> BValue {
        BValue { value, flag }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Default, Copy)]
pub struct RValue {
    pub rank: Rank, 
    pub value: i64,
}

impl RValue {
    pub fn new(rank: Rank, value: i64) -> RValue {
        RValue { rank, value }
    }
}

impl PartialOrd for RValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RValue {
    fn cmp(&self, other: &Self) -> Ordering {
        // First compare by rank
        match self.rank.cmp(&other.rank) {
            Ordering::Equal => self.value.cmp(&other.value), // If ranks are equal, compare by value
            ordering => ordering,                     // Otherwise, use the rank ordering
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RState {
    pub r_value: RValue, 
    pub broadcast: Broadcast,
}

impl RState {
    pub fn new(r_value: RValue, broadcast: Broadcast) -> RState {
        RState { r_value, broadcast}
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct AState {
    pub a_value: AValue, 
    pub broadcast: Broadcast,
}

impl AState {
    pub fn new(a_value: AValue, broadcast: Broadcast) -> AState {
        AState { a_value, broadcast }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct BState {
    pub b_value: BValue, 
    pub broadcast: Broadcast,
}

impl BState {
    pub fn new(b_value: BValue, broadcast: Broadcast) -> BState {
        BState { b_value, broadcast }
    }
}   

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Decision {
    Adopt(i64),
    Commit(i64),
}