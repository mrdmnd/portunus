This document describes the author's original implementation approach for the
simulation engine.

At the top level, we have a single `Engine` instance that maintains a thread
pool, a count of completed iterations, and a work queue. Conceptually, we've got
some target error we're trying to hit, and we want our threads to keep pulling
work off of the queue while the count of completed iterations doesn't exceed
some max value.

The engine keeps track of a (threadsafe) DPS error, which the threads update as
they finish a single iteration. The core of the engine loop essentially looks
like:

```
// While we're above target error and have no work pending,
while(error > target_error && current_iters < max_iters && queue.isEmpty()) {

  // Insert some new work into the queue, and let the threads race to grab it.
  // "work" here is a simple struct describing the unique parameters of the
  // iteration, possibly chosen at random. For instance, we want to pass the
  // length of the iteration as a parameter, and we want to sample these from a 
  // distribution that we are passed a priori.
  work_queue.insert( simulation_duration );
}
```

Each thread maintains a statistics counter which holds the DPS distribution from
all of the iterations that it's handled. There's also a contant "reporting
threshold" which describes a number of iterations after which the thread will
"report back" its statistics to the engine, which is in-turn used to update the
current dps error value.

Once the threads are all done handling their work, we merge their statistics
into a final simulation report.


Each thread instance maintains 
