//! Handling of the fractional scaling.

use crate::application::Application;
use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle, delegate_dispatch};
use sctk::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use sctk::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{
    Event as FractionalScalingEvent, WpFractionalScaleV1,
};

/// The scaling factor denominator.
const SCALE_DENOMINATOR: f64 = 120.;

/// Fractional scaling manager.
#[derive(Debug)]
pub struct FractionalScalingManager {
    manager: WpFractionalScaleManagerV1,
}

pub struct FractionalScaling {
    /// The surface used for scaling.
    surface: WlSurface,
}

impl FractionalScalingManager {
    /// Create new viewporter.
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<Application>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { manager })
    }

    pub fn fractional_scaling(
        &self,
        surface: &WlSurface,
        queue_handle: &QueueHandle<Application>,
    ) -> WpFractionalScaleV1 {
        let data = FractionalScaling {
            surface: surface.clone(),
        };
        self.manager
            .get_fractional_scale(surface, queue_handle, data)
    }
}

impl Dispatch<WpFractionalScaleManagerV1, GlobalData, Application> for FractionalScalingManager {
    fn event(
        _: &mut Application,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Application>,
    ) {
        // No events.
    }
}

impl Dispatch<WpFractionalScaleV1, FractionalScaling, Application> for FractionalScalingManager {
    fn event(
        state: &mut Application,
        _: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        data: &FractionalScaling,
        _: &Connection,
        _: &QueueHandle<Application>,
    ) {
        if let FractionalScalingEvent::PreferredScale { scale } = event {
            state.scale_factor_changed(&data.surface, scale as f64 / SCALE_DENOMINATOR, false);
        }
    }
}

delegate_dispatch!(Application: [WpFractionalScaleManagerV1: GlobalData] => FractionalScalingManager);
delegate_dispatch!(Application: [WpFractionalScaleV1: FractionalScaling] => FractionalScalingManager);
