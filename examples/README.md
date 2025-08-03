# `bigworlds` examples

This directory holds a set of example models. Each of the models tries to
illustrate different features of the `bigworlds` engine.

If you're completely new to `bigworlds` modeling, the `tour` model is a great
starting point. General documentation with getting started guides is also
available at the project website.

All models are provided with their respective documentation. For some of the
examples visualization layer is also available.


## Getting started

First you'll need to get the bigworlds tooling.

Install the `bigworlds` CLI using Cargo.

```
cargo install bigworlds-cli
```

Clone the repository if you haven't already and navigate to the relevant
directory:

```
git clone https://github.com/bigworlds-net/bigworlds
cd bigworlds/examples
```

Now you should be able to run models locally with the `bigworlds` CLI tool by
providing the model path:

```
bigworlds run [model_path]
```

You can also navigate to the model directory and just do:

```
bigworlds run
```

If the model defines multiple scenario targets you can also specify the
scenario by name:

```
bigworlds run [model_path] --scenario [scenario_name]
```
