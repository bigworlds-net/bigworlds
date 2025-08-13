use crate::machine::cmd::CommandResult;
use crate::machine::{Machine, Result};
use crate::SimHandle;

/// Invoke
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct Invoke {
    pub events: Vec<String>,
}
impl Invoke {
    pub fn new(args: Vec<String>) -> Result<Self> {
        let mut events = Vec::new();
        for arg in args {
            events.push(arg);
        }
        Ok(Invoke { events })
    }
}
impl Invoke {
    pub async fn execute(&self, machine: Machine) -> CommandResult {
        let events = self.events.clone();
        tokio::spawn(async move {
            machine
                .invoke(events)
                .await
                .inspect_err(|e| println!("{e}"));
        });
        CommandResult::Continue
    }
    // pub fn execute_ext(&self, sim: &mut SimHandle) -> Result<()> {
    //     for event in &self.events {
    //         if !sim.event_queue.contains(event) {
    //             sim.event_queue.push(event.to_owned());
    //         }
    //     }
    //     Ok(())
    // }
    // pub fn execute_ext_distr(&self, sim: &mut SimHandle) -> Result<()> {
    //     for event in &self.events {
    //         if !sim.event_queue.contains(event) {
    //             sim.event_queue.push(event.to_owned());
    //         }
    //     }
    //     Ok(())
    // }
}
