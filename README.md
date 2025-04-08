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
