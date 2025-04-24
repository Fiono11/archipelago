# Archipelago BFT Leaderless Consensus Algorithm

This codebase is a PoC implementation of the BFT-Archipelago leaderless consensus algorithm (Algorithm 5) presented in this [technical report](https://infoscience.epfl.ch/entities/publication/dcd9f339-a6fc-4b35-a3a3-beda85890adb).

## Preliminares
- There are n = 3f+1 processes, f byzantine
- 2f+1 processes are correct, which means they follow the algorithm f processes can behave arbitrarily (not respond, send arbitrary messages, etc), except impersonating other processes 
- Each process starts by proposing a value
- The algorithm terminates when all correct processes commit the same value 


## Rank and Steps
- Each process starts at rank 0
- Each rank  is composed of three steps: R, A and B
- If, after step B of rank i, the process has not committed yet, a new rank i+1 starts with step R, until a value is committed


## Values
- Each value is used in its corresponding step
- The R value is a tuple (rank, value)
- The A value is a single value
- The B value is a tuple (bool, value)


## Registers
- Each process has three registers: R, A and B
- Register R contains the maximum R value the process has seen, ordered first by rank then by value
- Register A is a sequence of sets of A values
- A[i] is the set of values of rank i
- Each A[i] can have at most two A values, the greatest ones
- Register B is a sequence of sets of B values
- B[i] is the set of values of rank i
- Each B[i] can have at most two B values, a true pair and a false pair


## Messages
### Broadcast
- The initial message that each process broadcasts at the start of each step (only one broadcast is sent per process per rank and step)
- It contains a label with the step (R, A or B), the rank i, the value v, an optional bool (for the B step) and an optional certificate
- A certificate contains 2f+1 responses of the previous step (except step R of rank 0)
- Example: the broadcast of step A and rank i must contain 2f+1 responses of step R and rank i 

### Response
- Every time a process receives a broadcast, it updates (or not) its own register and responds with its value(s) and the broadcast(s) that justify(ies) the value(s) 
- It contains a label indicating the step (Resp, Aresp or Bresp), the rank, the corresponding register, a signature and a certificate
- Example: a process receives a broadcast in step A with a different value than it has in register A, it adds that value to A and responds with the register A and the certificate, which contains the broadcasts that justify both values (one would be the broadcast it has just received and the other a previous one)


### Step R
- Each process starts with an empty register R
- Each process broadcasts its register R, value, rank and certificate (2f+1 responses of the previous step B, if rank i > 0)
- Every time a process receives a broadcast in rank i, it updates R[i] if the new value is greater
- It responds with the register R and the broadcast that justifies R[i]
- When the process receives 2f+1 R responses, it keeps the greatest value in R[i], returns that value and proceeds to step A


### Step A
- Each process starts with an empty register A
- Each process broadcasts its register A, value, rank and certificate (2f+1 responses of the previous step R)
- Every time a process receives a broadcast in rank i, it updates (or not) A[i] so that it contains at most the two greatest received values
- It responds with the register A and the broadcast(s) that justify(ies) A[i]
- When the process receives 2f+1 A responses, it returns (true, val) if the registers of the responses contain only a single value val, and (false, max(vals)) otherwise, and proceeds to step B


### Step B
- Each process starts with an empty register B
- Each process broadcasts its register B, value, rank and certificate (2f+1 responses of the previous step A)
- Every time a process receives a broadcast in rank i, it updates (or not) so that it contains at most a true pair and a false pair with the greatest value
- It responds with the register B and the broadcast(s) that justify(ies) B[i]
- When the process receives 2f+1 B responses, it returns (adopt, val with most counts or greatest) and proceeds to step R of rank i+, or it returns (commit, val) if the registers of the responses contain only true pairs and the algorithm terminates

## TODO
Lines missing:
- Line 77: check signatures of those messages 
- Line 78: check if |{bcast-answers}| > f -> at least f responses have a valid signature
- In a real implementation every message should be signed, so this should not be needed?

Complementing certificate:
- A complementing certificate at pi to a partial certificate for a broadcast (resp. response) comprises f + 1 (resp. 2f + 1) responses received by pj to each of the queries comprised in the partial certificate. 
- The reason why this complementing certificate contains more responses is to avoid waiting for f + 1 responses that are never received (e.g., responses responding to a broadcast sent by a malicious broadcaster). 
- Waiting for 2f +1 responses before considering the response valid guarantees that other correct processes eventually receive f + 1 responses. 

