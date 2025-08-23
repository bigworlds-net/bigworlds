use crate::net::CompositeAddress;

use super::DistributionPolicy;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub listeners: Vec<CompositeAddress>,

    /// Perform automatic stepping through the simulation, initiated on the
    /// leader level.
    pub autostep: Option<std::time::Duration>,

    /// Policy for entity distribution. Most policies involve a dynamic process
    /// of reassigning entities between workers to optimize for different
    /// factors.
    pub distribution: DistributionPolicy,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listeners: vec![],
            autostep: None,
            distribution: DistributionPolicy::Random,
        }
    }
}
