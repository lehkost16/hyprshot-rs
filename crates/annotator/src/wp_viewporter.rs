//! Handling of the wp-viewporter.

use crate::application::Application;
use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle, delegate_dispatch};
use sctk::reexports::protocols::wp::viewporter::client::wp_viewport::WpViewport;
use sctk::reexports::protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

/// Viewporter.
#[derive(Debug)]
pub struct ViewporterState {
    viewporter: WpViewporter,
}

impl ViewporterState {
    /// Create new viewporter.
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<Application>,
    ) -> Result<Self, BindError> {
        let viewporter = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { viewporter })
    }

    /// Get the viewport for the given object.
    pub fn get_viewport(
        &self,
        surface: &WlSurface,
        queue_handle: &QueueHandle<Application>,
    ) -> WpViewport {
        self.viewporter
            .get_viewport(surface, queue_handle, GlobalData)
    }
}

impl Dispatch<WpViewporter, GlobalData, Application> for ViewporterState {
    fn event(
        _: &mut Application,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Application>,
    ) {
        // No events.
    }
}
impl Dispatch<WpViewport, GlobalData, Application> for ViewporterState {
    fn event(
        _: &mut Application,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Application>,
    ) {
        // No events.
    }
}

delegate_dispatch!(Application: [WpViewporter: GlobalData] => ViewporterState);
delegate_dispatch!(Application: [WpViewport: GlobalData] => ViewporterState);
