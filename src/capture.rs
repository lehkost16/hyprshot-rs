#![allow(dead_code)]

use anyhow::{Context, Result};
use serde_json::Value;
use std::{collections::HashSet, process::Command, time::Duration};

use crate::geometry::Geometry;
use crate::selector;
use crate::utils::output_with_timeout;

pub struct HyprctlCache {
    monitors: Option<Value>,
}

impl HyprctlCache {
    pub fn new() -> Self {
        Self { monitors: None }
    }
}

fn hyprctl_monitors_json(cache: &mut HyprctlCache, timeout: Duration) -> Result<&Value> {
    if cache.monitors.is_none() {
        let output = output_with_timeout(
            {
                let mut cmd = Command::new("hyprctl");
                cmd.arg("monitors").arg("-j");
                cmd
            },
            timeout,
        )
        .context("Failed to run hyprctl monitors")?;
        let monitors: Value =
            serde_json::from_slice(&output.stdout).context("Failed to parse hyprctl monitors")?;
        cache.monitors = Some(monitors);
    }

    cache
        .monitors
        .as_ref()
        .context("Hyprctl monitors cache missing")
}

// Support matrix:
// - region/output: Wayland-wide via slurp-rs API
// - output by name: Wayland enumeration (no hyprctl)
// - window/active: Hyprland and Sway (hyprctl/swaymsg)
pub fn grab_active_output(debug: bool, cache: &mut HyprctlCache) -> Result<Geometry> {
    if let Ok(geometry) = grab_active_output_hyprctl(debug, cache) {
        return Ok(geometry);
    }
    if let Ok(geometry) = grab_active_output_sway(debug) {
        return Ok(geometry);
    }

    Err(anyhow::anyhow!(
        "Active output is only supported on Hyprland or Sway"
    ))
}

fn grab_active_output_hyprctl(debug: bool, cache: &mut HyprctlCache) -> Result<Geometry> {
    const IPC_TIMEOUT: Duration = Duration::from_secs(3);
    let active_workspace: Value = serde_json::from_slice(
        &output_with_timeout(
            {
                let mut cmd = Command::new("hyprctl");
                cmd.arg("activeworkspace").arg("-j");
                cmd
            },
            IPC_TIMEOUT,
        )
        .context("Failed to run hyprctl activeworkspace")?
        .stdout,
    )?;
    let monitors = hyprctl_monitors_json(cache, IPC_TIMEOUT)?;

    if debug {
        eprintln!("Monitors: {}", monitors);
        eprintln!("Active workspace: {}", active_workspace);
    }

    let current_monitor = monitors
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|m| m["activeWorkspace"]["id"] == active_workspace["id"])
        })
        .context("No matching monitor found")?;

    if debug {
        eprintln!("Current output: {}", current_monitor);
    }

    let x = current_monitor["x"].as_i64().unwrap_or(0) as i32;
    let y = current_monitor["y"].as_i64().unwrap_or(0) as i32;
    let width = current_monitor["width"].as_i64().unwrap_or(0) as f64;
    let height = current_monitor["height"].as_i64().unwrap_or(0) as f64;
    let scale = current_monitor["scale"].as_f64().unwrap_or(1.0);

    let geometry = Geometry::new(
        x,
        y,
        (width / scale).round() as i32,
        (height / scale).round() as i32,
    )?;
    if debug {
        eprintln!("Active output geometry: {}", geometry);
    }
    Ok(geometry)
}

fn grab_active_output_sway(debug: bool) -> Result<Geometry> {
    let workspaces = sway_msg(&["-t", "get_workspaces"])?;
    let focused_output = workspaces
        .as_array()
        .and_then(|arr| arr.iter().find(|w| w["focused"].as_bool() == Some(true)))
        .and_then(|w| w["output"].as_str())
        .context("Failed to find focused workspace output")?;

    let outputs = sway_msg(&["-t", "get_outputs"])?;
    let output_data = outputs
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|o| o["name"].as_str() == Some(focused_output))
        })
        .context("Focused output not found in sway outputs")?;

    let rect = output_data["rect"]
        .as_object()
        .context("Invalid output rect data")?;

    let x = rect.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
    let y = rect.get("y").and_then(|v| v.as_i64()).unwrap_or(0);
    let width = rect.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
    let height = rect.get("height").and_then(|v| v.as_i64()).unwrap_or(0);

    let geometry = Geometry::new(x as i32, y as i32, width as i32, height as i32)?;
    if debug {
        eprintln!("Active output geometry (sway): {}", geometry);
    }
    Ok(geometry)
}

pub fn grab_region(debug: bool) -> Result<Geometry> {
    selector::select_region(debug)
}

pub fn is_region_selection_cancelled(err: &anyhow::Error) -> bool {
    selector::is_cancelled(err, selector::SelectionTarget::Region)
}

pub fn grab_window(debug: bool, cache: &mut HyprctlCache) -> Result<Geometry> {
    if let Ok(geometry) = grab_window_hyprctl(debug, cache) {
        return Ok(geometry);
    }
    if let Ok(geometry) = grab_window_sway(debug) {
        return Ok(geometry);
    }

    Err(anyhow::anyhow!(
        "Window selection is only supported on Hyprland or Sway"
    ))
}

fn grab_window_hyprctl(debug: bool, cache: &mut HyprctlCache) -> Result<Geometry> {
    const IPC_TIMEOUT: Duration = Duration::from_secs(3);
    let monitors = hyprctl_monitors_json(cache, IPC_TIMEOUT)?;
    let clients: Value = serde_json::from_slice(
        &output_with_timeout(
            {
                let mut cmd = Command::new("hyprctl");
                cmd.arg("clients").arg("-j");
                cmd
            },
            IPC_TIMEOUT,
        )
        .context("Failed to run hyprctl clients")?
        .stdout,
    )?;

    let workspace_ids: HashSet<i64> = monitors
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["activeWorkspace"]["id"].as_i64())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();

    let filtered_clients: Vec<Value> = clients
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c["workspace"]["id"]
                        .as_i64()
                        .map(|id| workspace_ids.contains(&id))
                        .unwrap_or(false)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    if debug {
        eprintln!("Monitors: {}", monitors);
        eprintln!("Clients: {}", serde_json::to_string(&filtered_clients)?);
    }

    let boxes: String = filtered_clients
        .into_iter()
        .filter_map(|c| {
            let at = c["at"].as_array()?;
            let size = c["size"].as_array()?;
            let x = at[0].as_i64()?;
            let y = at[1].as_i64()?;
            let width = size[0].as_i64()?;
            let height = size[1].as_i64()?;
            if width <= 0 || height <= 0 {
                return None;
            }
            Some(format!(
                "{},{} {}x{} {}",
                x,
                y,
                width,
                height,
                c["title"].as_str().unwrap_or("")
            ))
        })
        .collect::<Vec<_>>()
        .join("\n");

    if debug {
        eprintln!("Window boxes:\n{}", boxes);
    }

    if boxes.is_empty() {
        return Err(anyhow::anyhow!("No valid windows found to capture"));
    }

    selector::select_from_boxes(&boxes, debug)
}

fn grab_window_sway(debug: bool) -> Result<Geometry> {
    let workspaces = sway_msg(&["-t", "get_workspaces"])?;
    let visible_workspaces: HashSet<String> = workspaces
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|w| w["visible"].as_bool() == Some(true))
                .filter_map(|w| w["name"].as_str().map(|s| s.to_string()))
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();

    let tree = sway_msg(&["-t", "get_tree"])?;
    let mut boxes = Vec::new();
    collect_visible_windows(&tree, &visible_workspaces, false, &mut boxes);

    if debug {
        eprintln!("Sway window boxes:\n{}", boxes.join("\n"));
    }

    if boxes.is_empty() {
        return Err(anyhow::anyhow!("No valid windows found to capture (sway)"));
    }

    selector::select_from_boxes(&boxes.join("\n"), debug)
}

fn collect_visible_windows(
    node: &Value,
    visible_workspaces: &HashSet<String>,
    mut visible: bool,
    boxes: &mut Vec<String>,
) {
    if node["type"].as_str() == Some("workspace") {
        visible = node
            .get("name")
            .and_then(|v| v.as_str())
            .map(|name| visible_workspaces.contains(name))
            .unwrap_or(false);
    }

    if visible
        && is_window_node(node)
        && let Some(line) = format_window_box(node)
    {
        boxes.push(line);
    }

    if let Some(nodes) = node.get("nodes").and_then(|v| v.as_array()) {
        for child in nodes {
            collect_visible_windows(child, visible_workspaces, visible, boxes);
        }
    }
    if let Some(nodes) = node.get("floating_nodes").and_then(|v| v.as_array()) {
        for child in nodes {
            collect_visible_windows(child, visible_workspaces, visible, boxes);
        }
    }
}

fn is_window_node(node: &Value) -> bool {
    if node["type"].as_str() != Some("con") {
        return false;
    }
    let has_app = node["app_id"].is_string();
    let has_props = node
        .get("window_properties")
        .map(|v| v.is_object())
        .unwrap_or(false);
    has_app || has_props
}

fn format_window_box(node: &Value) -> Option<String> {
    let rect = node.get("rect")?.as_object()?;
    let x = rect.get("x")?.as_i64()? as i32;
    let y = rect.get("y")?.as_i64()? as i32;
    let width = rect.get("width")?.as_i64()? as i32;
    let height = rect.get("height")?.as_i64()? as i32;
    if width <= 0 || height <= 0 {
        return None;
    }
    let title = node
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .replace('\n', " ");
    Some(format!("{},{} {}x{} {}", x, y, width, height, title))
}

fn sway_msg(args: &[&str]) -> Result<Value> {
    const IPC_TIMEOUT: Duration = Duration::from_secs(3);
    let output = output_with_timeout(
        {
            let mut cmd = Command::new("swaymsg");
            cmd.args(args);
            cmd
        },
        IPC_TIMEOUT,
    )
    .context("Failed to run swaymsg")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "swaymsg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).context("Failed to parse swaymsg JSON")
}
