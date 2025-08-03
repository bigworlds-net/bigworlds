# `boids`

This example model focuses on recreating the artificial life program as
originally devised by Craig Reynolds back in 1986.

We define a 2D world inhabited by `boid` entities. Individual `boid`s observe
their immediate environment and based on a number of simple rules modify their
internal state and future behavior.

As the simulation time progresses, intricate flocking behavior can be observed.


## Options

The model exposes some common variables for controlling the default behavior
of individual `boid`s.

Multiple scenarios are provided, providing different seed data and behavior
rule-sets.


## Scaling

The model can be executed either on a single machine or on clusters of
arbitrary size.

Running on a cluster spanning multiple machines, any individual `boid` can be
seamlessly moved around by the runtime to any worker. This moving of entities
between workers is done based on the chosen distribution policy, e.g.
targetting best performance overall, most spatial coherence, least network
traffic generated, etc.


## Further considerations

`boid` behavior could perhaps be made even more interesting if we decided to
give individual `boid`s more intelligence. That is, they could be building
internal models of the world and try predicting the near future based on those
models.

 

