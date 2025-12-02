pub mod configuration;
pub mod core;
pub mod routes;
pub mod startup;

pub use configuration::Config;
pub use core::State;
pub use startup::Application;

#[doc(hidden)]
pub mod testing {
    pub use crate::startup::poll;
}
