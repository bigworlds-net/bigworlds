# `cubes`

This example focuses on a simple physics-based cube stacking.

The way the initial layout of the cubes is done means the tower is fairly
unstable and will collapse within a finite number of steps. This gives us
a neatly limited scenario, time- and complexity- wise.


## `service`-based physics

Using an external physics solver we showcase the `service` layer functionality.

We can attach arbitrary executables to be part of our model as managed
`service`s. The only requirement is that the service must utilize the standard
client message-passing interface to connect to the exposed server.

Services can be useful for providing arbitrary logic to mutate simulation
state, though they also come with a substantial overhead cost. In cases where
performance is more important than model development flexibility, one can
provide logic through the `behavior` interface. It's also possible to develop
custom implementation of a worker that would incorporate necessary
model-related logic, albeit this comes at the cost of losing much of the
runtime flexibility in terms of model setup.

