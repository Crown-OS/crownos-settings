//! The two names `compositor.ron` may file a monitor under: its connector,
//! then the identity the compositor gave it.

use crownos_config::schema::{Compositor, OutputSetting};

use crate::net::outputs::Monitor;

#[derive(Debug, Clone, PartialEq)]
pub struct OutputNames {
    connector: String,
    identity: String,
}

impl OutputNames {
    /// `None` while the compositor has not given the monitor its identity.
    pub fn of(monitor: &Monitor) -> Option<Self> {
        Some(Self {
            connector: monitor.name.clone(),
            identity: monitor.identity()?.to_owned(),
        })
    }

    /// The name a new `primary_output` gets.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn setting<'a>(&self, compositor: &'a Compositor) -> Option<&'a OutputSetting> {
        compositor.output(&self.connector, &self.identity)
    }

    pub fn entry<'a>(&self, compositor: &'a mut Compositor) -> &'a mut OutputSetting {
        compositor.output_entry(&self.connector, &self.identity)
    }
}

#[cfg(test)]
pub fn names(connector: &str, identity: &str) -> OutputNames {
    OutputNames {
        connector: connector.to_owned(),
        identity: identity.to_owned(),
    }
}
