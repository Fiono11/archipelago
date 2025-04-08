# Overview 
This codebase is a PoC implementation of the BFT-Archipelago leaderless consensus algorithm (Algorithm 5) presented in this [technical report](https://infoscience.epfl.ch/entities/publication/dcd9f339-a6fc-4b35-a3a3-beda85890adb).

# Leaderless Consensus
- There are $n=3f+1$ processes, $f$ byzantine. 
- The algorithm progresses in ranks.
- Each rank has 3 steps: $R$, $A$ and $B$.
- (Page 7) In each round $r$, every (correct, non-suspended) a process $p_i$:
  1. Broadcasts a message (called a request).
  2. Delivers all requests that were sent to $p_i$ in $r$.
  3. Sends a message (called a response) for every request it has delivered in (ii).
  4. Delivers all replies sent to it in $r$.
- Every broadcast and response must have a valid certificate.
- A valid broadcast certificate must contain valid responses from $2f+1$ processes of the previous step (except step $R$ of rank 0, which does not need a certificate), and those the value of the broadcast must be in accordance with those responses.
- For a response to be valid, each of the broadcasts that justify the response must have received at least $2f+1$ responses.

# TODO
Lines missing:
- Line 77: check signatures of those messages 
- Line 78: check if |{bcast-answers}| > f -> at least f responses have a valid signature
- In a real implementation every message should be signed, so this should not be needed?

Complementing certificate:
- A complementing certificate at pi to a partial certificate for a broadcast (resp. response) comprises f + 1 (resp. 2f + 1) responses received by pj to each of the queries comprised in the partial certificate. 
- The reason why this complementing certificate contains more responses is to avoid waiting for f + 1 responses that are never received (e.g., responses responding to a broadcast sent by a malicious broadcaster). 
- Waiting for 2f +1 responses before considering the response valid guarantees that other correct processes eventually receive f + 1 responses. 

