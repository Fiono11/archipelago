# Overview 
This codebase is a PoC implementation of the BFT-Archipelago leaderless consensus algorithm (Algorithm 5) presented in this [technical report](https://infoscience.epfl.ch/entities/publication/dcd9f339-a6fc-4b35-a3a3-beda85890adb).

# Leaderless Consensus
- There are n=3f+1 processes, f byzantine. 
- The algorithm progresses in ranks.
- Each rank has 3 steps: R, A and B.
In each round r, every (correct, non-suspended) a process p_i (Page 7):
(i) broadcasts a message (called a request).
(ii) delivers all requests that were sent to p_i in r.
(iii) sends a message (called a response) for every request it has delivered in (ii).
(iv) delivers all replies sent to it in r.