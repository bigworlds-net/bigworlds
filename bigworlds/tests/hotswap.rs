//! Small showcase of the model-based logic hot-swapping functionality.
//!
//! Model-based means we utilize the ability to mutate the global model at
//! runtime and expect some side-effects to take place, such as the respawning
//! of the behavior tasks.
//!
//!
//! # Non-model-based swapping
//!
//! Hot-swapping logic for arbitrary behaviors we hold handles to is easier
//! still, see the `behavior` example.

use bigworlds::model::behavior::BehaviorInner;

mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging.
    env_logger::init();

    // Spawn local sim instance and keep the handle.
    let mut sim = bigworlds::sim::spawn_from_model(common::model()).await?;

    sim.step_by(3).await?;

    // Mutate all the lua behavior models, changing their script values to
    // a silly print.
    let mut model = common::model();
    model.behaviors.iter_mut().for_each(|b| {
        if let BehaviorInner::Lua { script, .. } = &mut b.inner {
            *script = r#"print("swapped!")"#.to_string();
        }
    });

    assert_ne!(common::model(), model);

    // Hot-pull the model containing modified behaviors.
    //
    // When pulling the model a diff will be performed, checking for changes
    // in behavior models between old and new version. If a change is detected
    // all behavior tasks already running on particular worker that are based
    // on the same behavior model will be shut down and restarted using the
    // new behavior model.
    sim.pull_model(model).await?;

    // We should now be running the changed lua behaviors.
    sim.step_by(3).await?;

    Ok(())
}
