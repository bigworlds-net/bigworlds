# Design overview

Everything in the simulation world is represented as an `entity`. Each entity is
made up of `component`s, which provide the data definitions. The simulation
data model is always based on composition of components attached to entities.

Entities can be freely moved between machines to continuously improve
efficiency of the system as a whole. Due to the dynamic nature of this
arrangement, internally entities are not represented as simply UUIDs as they
would be in a classic ECS design. They are instead more involved structures
with their own storages. Entities themselves are stored in an addressed map
for quick retrieval.

Components are also different from what one would see in a classic ECS design.
Due to the interoperability requirements, components are not defining the
actual memory layouts for particular data. Data representation is fixed on the
lower individual data type layer (e.g. numbers, strings). Components are
defined as collections of variables, the layout of which will be determined 
dynamically on the entity storage layer.

The above entity-component design can be described as the storage layer. It's
usable as is. One can define the model and starting state through `.toml` files
and deploy to an existing bigworlds cluster. Existing simulation data can be
queried and mutated, and upon proper configuration the entity distribution
across the cluster will dynamically approach most efficient arrangement based
on the access patterns.

Building on the storage layer is the logic layer.

Bigworlds runtime builds directly on the tokio runtime, spawning separate tasks
representing different parts of the system.

One part of the system spawned as a separate task is the `behavior`. That's
basically a free-form task running on the tokio runtime. Each behavior unit
is a separate task performing read/write operations asynchronously, no matter
if they're performed locally or remotely (though the system is likely
constantly optimizing to maximize the number of local operations). Processing
can optionally be synchronized with simulation-wide events.

Spawning a behavior task is the most direct way of introducing logic into
a bigworlds system. These tasks can be defined as part of the target program
using bigworlds library, or they can be loaded dynamically onto existing
bigworlds systems. Since it's not exactly safe in situations where hardware is
shared, we definitely want to have a way to sandbox user provided logic. For
this we can use different kind of scripting behaviors.

There's also a concept of a slightly more abstract dsl-based  `machine`.
Machine is a behavior executing a set of predefined instructions in a certain
manner, like a *virtual machine*. With it's basic support for event-triggered
state changes, it can also resemble a *state machine*.

All in all, modelers can define logic in a variety of different ways, including
sandboxed scripting, ensuring safe execution on the bigworlds runtime alongside
the storage layer. This has an added benefit in terms of potentially driving
adoption among non-programmers (e.g. game modders).

One abstraction level higher are the logic processing modules called services.
Sevices use the client-server interface which relies entirely on message
passing, making them language-agnostic. Bigworlds runtime supports
declaring services in the model definition. Those are what we call managed
sevices. Declaration of a managed service provides relevant information such as
what components or component collections it's needed for, letting the runtime
ensure that all nodes hosting certain entities has got this type of service
running. Managed services can also exist in different flavours, e.g. they can
be expected to exist at all nodes or only at selected ones, driving the
eventual distribution of entities across machines. This is a nice feature that
can enable bigworlds clusters to be very flexible in terms of hardware makeup
(think android phones alongside powerful compute units). 

Unmanaged services are any external programs that connect to a running world
using the client-server interface. These could be some black-box proprietary
software for example, handling it's own part of the simulation with inputs and
outputs defined as part of a *service programming interface* of sorts. 


# Cluster layout

A bigworlds cluster is first and foremost composed of workers. Workers
collectively hold simulation state, which is made up of discrete entities.
They also execute logic that mutates the state. We can divide the workers 
as follows.

*Leader* worker is one that's additionally tasked with performing some
operations related to managing global state of the simulation. For example 
related to maintaining globally coherent simulation model without having to do
complex consensus negotiations between individual workers.

So called *storage* worker is worker unable to perform any simulation logic.
It's only tasked with storing entity data and retrieving it upon request.
It can be useful for some particular hardware configurations, for example when
we want to store a lot of *inanimate* entities in RAM or on disk.

*Runner* worker here means one performing simulation logic, either in form of
behaviors or custom service code.

*Server* worker is a node that provides the interface for peeking into the
cluster from the outside. 

Clients call into the server with various requests to query and mutate
simulation state. Server calls on it's partner worker to process whatever
it needs to fulfil the request. In turn the worker can query other workers
to get whatever data it needs to fulfil the request it got from the server.

```
       ______________________________________________________________
      |                   |                   |                      |
  worker (leader)       worker (storage)       worker (server)        worker (runner)
----------------------------------------------------------------------------
                                ______________|_______________
                               |                              |
                               |                              |
                            client (user)                  client (service)

```


# Client request flow

There are at least two degrees of separation when querying a world from
a client. Client is usually far from the server, so that's the biggest jump.
Server task is usually present on the same machine as worker, so at this point
we're much closer to the data. It's possible that the server task is sitting on
a *relay* worker though, or that client is requesting data not available on the
local worker anyway.

Based on what query was issued, processing it to completion can make multiple
additional steps involving mostly worker chatter. For example querying for all
entities with some specific set of components currently attached effectively
requires propagating that query among all workers.

Of course there are ways to scope the query such that only the initial-contact
worker will be considered, or that we only propagate the query n-times.
Scoping queries in such a way should take into consideration cluster topology
though, as it's totally possible to lay out the workers in a way that they will
only be connected to a single other worker for example.

```
                       [cluster boundary]
                               |
          1. sends request     |          2. forwards request            3. requests data
client <---------------------> | server <---------------------> worker <------------------> worker
         6. returns response   |          5. returns response             4. returns data
                               |    
```


