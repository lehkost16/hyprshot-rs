use anyhow::Result;
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

use crate::geometry::Geometry;
use crate::utils::output_with_timeout;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MonitorInfo {
    pub name: String,
    pub scale: f64,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn get_active_monitor_info(_debug: bool) -> Result<(String, f64, i32, i32)> {
    const IPC_TIMEOUT: Duration = Duration::from_secs(3);
    // Try Hyprland first
    if let Ok(output) = output_with_timeout(
        {
            let mut cmd = Command::new("hyprctl");
            cmd.arg("activeworkspace").arg("-j");
            cmd
        },
        IPC_TIMEOUT,
    ) && let Ok(active_workspace) = serde_json::from_slice::<Value>(&output.stdout)
        && let Ok(output_mon) = output_with_timeout(
            {
                let mut cmd = Command::new("hyprctl");
                cmd.arg("monitors").arg("-j");
                cmd
            },
            IPC_TIMEOUT,
        )
        && let Ok(monitors) = serde_json::from_slice::<Value>(&output_mon.stdout)
        && let Some(arr) = monitors.as_array()
        && let Some(m) = arr
            .iter()
            .find(|m| m["activeWorkspace"]["id"] == active_workspace["id"])
    {
        let name = m["name"].as_str().unwrap_or("").to_string();
        let scale = m["scale"].as_f64().unwrap_or(1.0);
        let x = m["x"].as_i64().unwrap_or(0) as i32;
        let y = m["y"].as_i64().unwrap_or(0) as i32;
        return Ok((name, scale, x, y));
    }

    // Try Sway
    if let Ok(output) = output_with_timeout(
        {
            let mut cmd = Command::new("swaymsg");
            cmd.arg("-t").arg("get_workspaces");
            cmd
        },
        IPC_TIMEOUT,
    ) && let Ok(workspaces) = serde_json::from_slice::<Value>(&output.stdout)
        && let Some(arr) = workspaces.as_array()
        && let Some(w) = arr.iter().find(|w| w["focused"].as_bool() == Some(true))
        && let Some(focused_output) = w["output"].as_str()
        && let Ok(output_mon) = output_with_timeout(
            {
                let mut cmd = Command::new("swaymsg");
                cmd.arg("-t").arg("get_outputs");
                cmd
            },
            IPC_TIMEOUT,
        )
        && let Ok(outputs) = serde_json::from_slice::<Value>(&output_mon.stdout)
        && let Some(arr_mon) = outputs.as_array()
        && let Some(o) = arr_mon
            .iter()
            .find(|o| o["name"].as_str() == Some(focused_output))
    {
        let name = o["name"].as_str().unwrap_or("").to_string();
        let scale = o["scale"].as_f64().unwrap_or(1.0);
        let rect = &o["rect"];
        let x = rect["x"].as_i64().unwrap_or(0) as i32;
        let y = rect["y"].as_i64().unwrap_or(0) as i32;
        return Ok((name, scale, x, y));
    }

    Ok(("eDP-1".to_string(), 1.0, 0, 0))
}

pub(crate) fn get_monitor_info_for_geometry(
    geometry: &Geometry,
    debug: bool,
) -> Result<MonitorInfo> {
    const IPC_TIMEOUT: Duration = Duration::from_secs(3);

    if let Ok(output) = output_with_timeout(
        {
            let mut cmd = Command::new("hyprctl");
            cmd.arg("monitors").arg("-j");
            cmd
        },
        IPC_TIMEOUT,
    ) && let Ok(monitors) = serde_json::from_slice::<Value>(&output.stdout)
        && let Some(monitor) = monitors
            .as_array()
            .and_then(|arr| find_monitor_for_geometry(arr, geometry, monitor_info_from_hypr))
    {
        if debug {
            eprintln!("Selected output for region: {:?}", monitor);
        }
        return Ok(monitor);
    }

    if let Ok(output) = output_with_timeout(
        {
            let mut cmd = Command::new("swaymsg");
            cmd.arg("-t").arg("get_outputs");
            cmd
        },
        IPC_TIMEOUT,
    ) && let Ok(outputs) = serde_json::from_slice::<Value>(&output.stdout)
        && let Some(monitor) = outputs
            .as_array()
            .and_then(|arr| find_monitor_for_geometry(arr, geometry, monitor_info_from_sway))
    {
        if debug {
            eprintln!("Selected output for region: {:?}", monitor);
        }
        return Ok(monitor);
    }

    let (name, scale, x, y) = get_active_monitor_info(debug)?;
    Ok(MonitorInfo {
        name,
        scale,
        x,
        y,
        width: i32::MAX,
        height: i32::MAX,
    })
}

fn find_monitor_for_geometry(
    outputs: &[Value],
    geometry: &Geometry,
    parse: fn(&Value) -> Option<MonitorInfo>,
) -> Option<MonitorInfo> {
    let center_x = geometry.x + geometry.width / 2;
    let center_y = geometry.y + geometry.height / 2;

    outputs.iter().filter_map(parse).find(|monitor| {
        center_x >= monitor.x
            && center_x < monitor.x + monitor.width
            && center_y >= monitor.y
            && center_y < monitor.y + monitor.height
    })
}

fn monitor_info_from_hypr(value: &Value) -> Option<MonitorInfo> {
    let scale = value["scale"].as_f64().unwrap_or(1.0);
    // Hyprland monitor dimensions are already logical compositor coordinates.
    // Dividing them by scale would make the captured image unnecessarily small.
    let width = value["width"].as_i64()? as i32;
    let height = value["height"].as_i64()? as i32;
    Some(MonitorInfo {
        name: value["name"].as_str().unwrap_or("").to_string(),
        scale,
        x: value["x"].as_i64().unwrap_or(0) as i32,
        y: value["y"].as_i64().unwrap_or(0) as i32,
        width,
        height,
    })
}

fn monitor_info_from_sway(value: &Value) -> Option<MonitorInfo> {
    let rect = &value["rect"];
    Some(MonitorInfo {
        name: value["name"].as_str().unwrap_or("").to_string(),
        scale: value["scale"].as_f64().unwrap_or(1.0),
        x: rect["x"].as_i64().unwrap_or(0) as i32,
        y: rect["y"].as_i64().unwrap_or(0) as i32,
        width: rect["width"].as_i64()? as i32,
        height: rect["height"].as_i64()? as i32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_hypr_monitor_containing_selection_center() {
        let outputs = vec![
            json!({
                "name": "eDP-1",
                "x": 0,
                "y": 0,
                "width": 2560,
                "height": 1600,
                "scale": 2.0
            }),
            json!({
                "name": "HDMI-A-1",
                "x": 1280,
                "y": 0,
                "width": 1920,
                "height": 1080,
                "scale": 1.0
            }),
        ];
        let geometry = Geometry::new(2600, 100, 300, 200).unwrap();

        let monitor = find_monitor_for_geometry(&outputs, &geometry, monitor_info_from_hypr)
            .expect("expected monitor containing region center");

        assert_eq!(monitor.name, "HDMI-A-1");
        assert_eq!(monitor.x, 1280);
        assert_eq!(monitor.width, 1920);
    }

    #[test]
    fn hypr_monitor_dimensions_are_logical() {
        let output = json!({
            "name": "eDP-1",
            "x": 0,
            "y": 0,
            "width": 2560,
            "height": 1600,
            "scale": 2.0
        });

        let monitor = monitor_info_from_hypr(&output).expect("expected monitor info");

        assert_eq!(monitor.width, 2560);
        assert_eq!(monitor.height, 1600);
    }
}
