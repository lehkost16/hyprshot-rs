use crate::annotator::free_line_based::{MarkerPenTool, PencilTool};
use crate::annotator::image_based::{
    BackgroundImageWithAnnotationsProvider, BlurHandler, BlurTool, EraserTool, ExtractHandler,
    MosaicHandler, MosaicTool, OriginalBackgroundImageProvider,
};
use crate::annotator::rectangle_based::{EllipseTool, RectangleTool};
use crate::annotator::serial_number::SerialNumberTool;
use crate::annotator::straight_line_based::{ArrowTool, StraightLineTool};
use crate::annotator::text::TextTool;
use crate::annotator::{
    AnnotationTool, AnnotatorState, ApplyExtraZoomFactor, ExtraZoomFactorSupport, FillColorSupport,
    FontColorSupport, SharedAnnotatorState, SharedAnnotatorStateUtil, StrokeColorSupport,
    StrokeWidthSupport, ToolName, WheelHandler,
};
use crate::context::Command;
use crate::egui_off_screen_render::EguiOffScreenRender;
use crate::global::{ReadGlobalMut, ReadOrInsertGlobal};
use crate::platform::application::Application;
use crate::platform::dpi::{LogicalPosition, PhysicalSize};
use crate::platform::view::ViewId;
use crate::platform::window::AppWindow;
use egui::load::SizedTexture;
use egui::{Area, Color32, ColorImage, Frame, Image, ImageSource, Order, Rect, Sense, pos2, vec2};
use image::RgbaImage;
use log::info;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub fn create_annotator_panel(
    view_id: ViewId,
    app: &mut Application,
    window: &mut AppWindow,
    image: Arc<RgbaImage>,
    logical_position: LogicalPosition<i32>,
    fit_scale: f32,
    scale_factor: f64,
) {
    let global_state = &app.global_state;

    // 使用 fit_scale 创建正确的初始面板大小，而不是始终用原始图片大小
    let panel_size = PhysicalSize::new(image.width(), image.height())
        .to_logical(scale_factor)
        .apply_extra_zoom_factor(fit_scale);

    window.create_sub_surface_view(
        view_id,
        global_state,
        panel_size,
        logical_position,
        Box::new(move |input, egui_ctx, app, window, current_view| {
            // 将图像数据上传到 GPU 并获取纹理句柄
            let image_width = image.width();
            let image_height = image.height();
            let annotator_state: &SharedAnnotatorState = window
                .window_context
                .globals_by_type
                .get_global_or_insert_with(|| {
                    // 创建 ColorImage
                    // 注意：RgbaImage 的 bytes 应该是连续的 RGBA 数据
                    let background_image = Arc::new(ColorImage::from_rgba_unmultiplied(
                        [image_width as usize, image_height as usize],
                        image.as_raw(),
                    ));
                    // Load the texture only once.
                    let texture_handle = egui_ctx.load_texture(
                        "background-image",
                        egui::ImageData::Color(background_image),
                        crate::texture::screenshot_texture_options(),
                    );

                    let renderer = Arc::new(EguiOffScreenRender::new(
                        app.global_state.gpu.borrow().as_ref().unwrap(),
                    ));

                    let annotator_state = AnnotatorState {
                        session: app.session.clone(),
                        extra_zoom_factor: fit_scale, // 初始化为适应屏幕的缩放比例
                        initial_zoom_factor: fit_scale,
                        background_image: image.clone(),
                        background_texture_handle: Some(texture_handle),
                        renderer: renderer.clone(),
                        annotation_tools: Default::default(),
                        annotations_stack: vec![],
                        redo_stack: vec![],
                        current_annotation_tool: None,
                        toolbar_visible: true,
                        marker_pen_straight_mode: app.session.settings().marker_pen_straight_mode,
                        candidate_colors: vec![
                            Color32::from_hex("#ff453a").unwrap(), // 鲜红
                            Color32::from_hex("#ff9f0a").unwrap(), // 活力橙
                            Color32::from_hex("#ffd60a").unwrap(), // 明黄
                            Color32::from_hex("#30d158").unwrap(), // 翠绿
                            Color32::from_hex("#64d2ff").unwrap(), // 青天蓝
                            Color32::from_hex("#0a84ff").unwrap(), // 极速蓝
                            Color32::from_hex("#bf5af2").unwrap(), // 魅紫
                            Color32::from_hex("#ff375f").unwrap(), // 玫瑰粉
                            Color32::from_hex("#ffffff").unwrap(), // 纯白
                            Color32::from_hex("#1c1c1e").unwrap(), // 深黑
                        ],
                    };

                    let annotator_state_rc = Rc::new(RefCell::new(annotator_state));
                    let rectangle_tool = RectangleTool::new(Rc::downgrade(&annotator_state_rc));
                    let ellipse_tool = EllipseTool::new(Rc::downgrade(&annotator_state_rc));
                    let straight_line_tool =
                        StraightLineTool::new(Rc::downgrade(&annotator_state_rc));
                    let arrow_tool = ArrowTool::new(Rc::downgrade(&annotator_state_rc));
                    let pencil_tool = PencilTool::new(Rc::downgrade(&annotator_state_rc));
                    let marker_pen_tool = MarkerPenTool::new(Rc::downgrade(&annotator_state_rc));
                    let mosaic_tool = MosaicTool::new(
                        Rc::downgrade(&annotator_state_rc),
                        Box::new(BackgroundImageWithAnnotationsProvider::new(
                            renderer.clone(),
                        )),
                        Rc::new(MosaicHandler::new(10)),
                    );
                    let blur_tool = BlurTool::new(
                        Rc::downgrade(&annotator_state_rc),
                        Box::new(BackgroundImageWithAnnotationsProvider::new(renderer)),
                        Rc::new(BlurHandler::default()),
                    );
                    let eraser_tool = EraserTool::new(
                        Rc::downgrade(&annotator_state_rc),
                        Box::new(OriginalBackgroundImageProvider::new()),
                        Rc::new(ExtractHandler::new()),
                    );
                    let text_tool = TextTool::new(Rc::downgrade(&annotator_state_rc));
                    let serial_number_tool =
                        SerialNumberTool::new(Rc::downgrade(&annotator_state_rc));

                    annotator_state_rc.borrow_mut().annotation_tools.insert(
                        ToolName::Rectangle,
                        AnnotationTool::Rectangle(rectangle_tool),
                    );
                    annotator_state_rc
                        .borrow_mut()
                        .annotation_tools
                        .insert(ToolName::Ellipse, AnnotationTool::Ellipse(ellipse_tool));
                    annotator_state_rc.borrow_mut().annotation_tools.insert(
                        ToolName::StraightLine,
                        AnnotationTool::StraightLine(straight_line_tool),
                    );
                    annotator_state_rc
                        .borrow_mut()
                        .annotation_tools
                        .insert(ToolName::Arrow, AnnotationTool::Arrow(arrow_tool));
                    annotator_state_rc
                        .borrow_mut()
                        .annotation_tools
                        .insert(ToolName::Pencil, AnnotationTool::Pencil(pencil_tool));
                    annotator_state_rc.borrow_mut().annotation_tools.insert(
                        ToolName::MarkerPen,
                        AnnotationTool::MarkerPen(marker_pen_tool),
                    );
                    annotator_state_rc
                        .borrow_mut()
                        .annotation_tools
                        .insert(ToolName::Mosaic, AnnotationTool::Mosaic(mosaic_tool));
                    annotator_state_rc
                        .borrow_mut()
                        .annotation_tools
                        .insert(ToolName::Blur, AnnotationTool::Blur(blur_tool));
                    annotator_state_rc
                        .borrow_mut()
                        .annotation_tools
                        .insert(ToolName::Eraser, AnnotationTool::Eraser(eraser_tool));
                    annotator_state_rc
                        .borrow_mut()
                        .annotation_tools
                        .insert(ToolName::Text, AnnotationTool::Text(text_tool));
                    annotator_state_rc.borrow_mut().annotation_tools.insert(
                        ToolName::SerialNumber,
                        AnnotationTool::SerialNumber(serial_number_tool),
                    );

                    let config = app.session.settings();
                    let mut state_mut = annotator_state_rc.borrow_mut();
                    for tool in state_mut.annotation_tools.values_mut() {
                        let tool_name = format!("{:?}", tool.tool_name());
                        let settings = config.tool_settings.get(&tool_name);
                        if tool.supports_set_stroke_width() {
                            let width = settings
                                .and_then(|settings| settings.stroke_width)
                                .unwrap_or_else(|| tool.stroke_width());
                            tool.set_stroke_width(width);
                        }
                        if tool.supports_set_stroke_color() {
                            let saved_color = settings
                                .and_then(|settings| settings.stroke_color_rgba)
                                .map(|rgba| {
                                    Color32::from_rgba_unmultiplied(
                                        rgba[0], rgba[1], rgba[2], rgba[3],
                                    )
                                })
                                .unwrap_or_else(|| tool.stroke_color());
                            let color = if tool.tool_name() == ToolName::MarkerPen {
                                Color32::from_rgba_unmultiplied(
                                    saved_color.r(),
                                    saved_color.g(),
                                    saved_color.b(),
                                    112,
                                )
                            } else {
                                saved_color
                            };
                            tool.set_stroke_color(color);
                        }
                        if tool.supports_set_fill_color() && !tool.supports_get_stroke_color() {
                            // 只有纯填充工具（无描边）才恢复填充色，对于带描边的矩形/椭圆，默认保持空心 (None)
                            let saved_fill = settings
                                .and_then(|settings| settings.fill_color_rgba)
                                .map(|rgba| {
                                    Color32::from_rgba_unmultiplied(
                                        rgba[0], rgba[1], rgba[2], rgba[3],
                                    )
                                })
                                .unwrap_or_else(|| {
                                    tool.fill_color().unwrap_or(Color32::TRANSPARENT)
                                });
                            if saved_fill.a() > 0 {
                                tool.set_fill_color(saved_fill);
                            }
                        }
                        if tool.supports_set_font_color() {
                            let saved_font = settings
                                .and_then(|settings| settings.font_color_rgba)
                                .map(|rgba| {
                                    Color32::from_rgba_unmultiplied(
                                        rgba[0], rgba[1], rgba[2], rgba[3],
                                    )
                                })
                                .unwrap_or_else(|| tool.font_color());
                            tool.set_font_color(saved_font);
                        }
                    }

                    // 荧光笔有独立的粗细与透明色语义，最后显式恢复一次，避免通用工具
                    // 初始化或旧版全局字段覆盖其专属配置。
                    if let Some(settings) = config.tool_settings.get("MarkerPen")
                        && let Some(tool) = state_mut.annotation_tools.get_mut(&ToolName::MarkerPen)
                    {
                        if let Some(width) = settings.stroke_width {
                            tool.set_stroke_width(width);
                        }
                        if let Some([r, g, b, _]) = settings.stroke_color_rgba {
                            tool.set_stroke_color(Color32::from_rgba_unmultiplied(r, g, b, 112));
                        }
                    }
                    drop(state_mut);
                    if config.auto_activate_default_tool {
                        let tool_name = ToolName::from_config_value(&config.active_tool)
                            .unwrap_or(ToolName::Rectangle);
                        annotator_state_rc
                            .borrow_mut()
                            .activate_annotation_tool(tool_name);
                    }

                    annotator_state_rc
                });

            // 将图像数据上传到 GPU 并获取纹理句柄
            let annotator_state = annotator_state.borrow_mut();
            let texture_handle = annotator_state.background_texture_handle.as_ref().unwrap();
            let texture_handle = texture_handle.clone();
            drop(annotator_state);

            // 构建 UI 的具体内容
            egui_ctx.run_ui(input, move |ctx| {
                egui::CentralPanel::default()
                    .frame(Frame::new().fill(Color32::TRANSPARENT))
                    .show(ctx, |ui| {
                        let bg_image = Image::new(ImageSource::Texture(SizedTexture::from_handle(
                            &texture_handle,
                        )));

                        let annotator_state = window
                            .window_context
                            .globals_by_type
                            .require_ref_mut::<SharedAnnotatorState>()
                            .clone();

                        let scale_factor = ui.ctx().pixels_per_point();
                        let extra_zoom_factor = annotator_state.borrow().extra_zoom_factor;
                        ui.ctx().set_extra_zoom_factor(extra_zoom_factor);

                        // 标注面板的逻辑大小=背景图片的逻辑大小=(背景图片的物理大小/scale_factor) * extra_zoom_factor)
                        let logical_size = PhysicalSize::new(image_width, image_height)
                            .to_logical(scale_factor as f64)
                            .apply_extra_zoom_factor_with_ctx(ui.ctx());

                        let panel_size = vec2(logical_size.width, logical_size.height);

                        bg_image.paint_at(ui, Rect::from_min_size(pos2(0., 0.), panel_size));

                        // 捕捉背景点击与拖拽手势
                        let bg_response = ui.allocate_rect(
                            Rect::from_min_size(pos2(0., 0.), panel_size),
                            Sense::click_and_drag(),
                        );

                        show_toolbar_toggle(
                            ui,
                            panel_size,
                            &mut annotator_state.borrow_mut().toolbar_visible,
                        );

                        annotator_state
                            .borrow_mut()
                            .annotations_stack
                            .iter_mut()
                            .for_each(|annotation| {
                                ui.add(annotation);
                            });

                        if annotator_state.borrow().current_annotation_tool.is_some() {
                            Area::new("annotation_tool_area".into())
                                .fixed_pos(pos2(0., 0.))
                                .order(Order::Middle)
                                .movable(false)
                                .interactable(true)
                                .show(ui, |ui| {
                                    ui.set_width(panel_size.x);
                                    ui.set_height(panel_size.y);
                                    annotator_state.with_current_annotation_tool(|tool| {
                                        ui.add(tool);
                                    })
                                });
                        }

                        // 随时处理滚轮缩放窗口
                        let zoom_before = annotator_state.borrow().extra_zoom_factor;
                        annotator_state.borrow_mut().handle_wheel_event(ui);
                        let zoom_after = annotator_state.borrow().extra_zoom_factor;

                        if zoom_before != zoom_after {
                            info!(
                                "extra_zoom_factor changed from {} to {}",
                                zoom_before, zoom_after
                            );

                            let current_img_w = annotator_state.borrow().background_image.width();
                            let current_img_h = annotator_state.borrow().background_image.height();
                            let new_size = PhysicalSize::new(current_img_w, current_img_h)
                                .to_logical(ui.ctx().pixels_per_point() as f64)
                                .apply_extra_zoom_factor(zoom_after);

                            info!("标注面板尺寸更改from {:?} to {:?}", logical_size, new_size);

                            window
                                .window_context
                                .commands
                                .push_back(Command::ResizeView(current_view.id(), new_size));

                            ui.ctx().request_repaint();
                        }

                        // 处理窗口拖动（无激活画笔时）
                        if annotator_state.borrow().current_annotation_tool.is_none()
                            && bg_response.dragged()
                        {
                            window
                                .window_context
                                .commands
                                .push_back(Command::StartMovingWindow);
                        }

                        // 处理全局快捷键
                        ui.ctx().input(|i| {
                            for event in &i.events {
                                if let egui::Event::Key {
                                    key,
                                    pressed: true,
                                    modifiers,
                                    ..
                                } = event
                                {
                                    let is_ctrl = modifiers.command || modifiers.ctrl;

                                    if *key == egui::Key::Escape {
                                        let (has_tool, toolbar_on) = {
                                            let s = annotator_state.borrow();
                                            (s.current_annotation_tool.is_some(), s.toolbar_visible)
                                        };
                                        if has_tool {
                                            annotator_state
                                                .borrow_mut()
                                                .deactivate_annotation_tool();
                                        } else if toolbar_on {
                                            annotator_state.borrow_mut().toolbar_visible = false;
                                        } else {
                                            window
                                                .window_context
                                                .commands
                                                .push_back(Command::CloseWindow);
                                        }
                                    }

                                    // 撤销: Ctrl+Z 或 Ctrl+U
                                    if ((*key == egui::Key::Z && is_ctrl && !modifiers.shift)
                                        || (*key == egui::Key::U && is_ctrl))
                                        && annotator_state.borrow().can_undo()
                                    {
                                        annotator_state.borrow_mut().undo();
                                    }

                                    // 重做: Ctrl+Y 或 Ctrl+Shift+Z 或 Ctrl+R
                                    if ((*key == egui::Key::Y && is_ctrl)
                                        || (*key == egui::Key::Z && is_ctrl && modifiers.shift)
                                        || (*key == egui::Key::R && is_ctrl))
                                        && annotator_state.borrow().can_redo()
                                    {
                                        annotator_state.borrow_mut().redo();
                                    }

                                    // 复制到剪贴板: Ctrl+C
                                    if *key == egui::Key::C && is_ctrl {
                                        let image_receiver = annotator_state
                                            .borrow_mut()
                                            .take_screenshot(ui.pixels_per_point());
                                        window.window_context.commands.push_back(
                                            Command::ExportImage(
                                                image_receiver,
                                                crate::export::ExportAction::Copy,
                                            ),
                                        );
                                    }

                                    // 保存: Ctrl+S
                                    if *key == egui::Key::S && is_ctrl {
                                        let image_receiver = annotator_state
                                            .borrow_mut()
                                            .take_screenshot(ui.pixels_per_point());
                                        window.window_context.commands.push_back(
                                            Command::ExportImage(
                                                image_receiver,
                                                crate::export::ExportAction::Save,
                                            ),
                                        );
                                    }
                                }
                            }
                        });
                    });
            })
        }),
        None,
    );
}

fn show_toolbar_toggle(ui: &mut egui::Ui, panel_size: egui::Vec2, expanded: &mut bool) -> Rect {
    let side = 28.0_f32.min(panel_size.x).min(panel_size.y).max(1.0);
    let margin = if panel_size.x >= 44.0 && panel_size.y >= 44.0 {
        8.0
    } else {
        0.0
    };
    Area::new("toolbar-toggle".into())
        .fixed_pos(pos2((panel_size.x - side - margin).max(0.0), margin))
        .order(Order::Foreground)
        .movable(false)
        .show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
            ui.painter().rect_filled(
                rect,
                6.0,
                if response.hovered() {
                    crate::ui::toolbar_style::HOVER
                } else {
                    crate::ui::toolbar_style::BACKGROUND
                },
            );
            let center = rect.center();
            let radius = side * 0.2;
            let stroke = egui::Stroke::new(1.5, Color32::WHITE);
            ui.painter().line_segment(
                [center - vec2(radius, 0.0), center + vec2(radius, 0.0)],
                stroke,
            );
            if !*expanded {
                ui.painter().line_segment(
                    [center - vec2(0.0, radius), center + vec2(0.0, radius)],
                    stroke,
                );
            }
            if response
                .on_hover_text(if *expanded {
                    "收起工具栏"
                } else {
                    "展开工具栏"
                })
                .clicked()
            {
                *expanded = !*expanded;
                ui.ctx().request_repaint();
            }
            rect
        })
        .inner
}

#[cfg(test)]
mod toolbar_toggle_tests {
    use super::*;

    #[test]
    fn toggle_stays_clickable_after_repeated_collapse_and_expand() {
        let ctx = egui::Context::default();
        let panel = vec2(720.0, 400.0);
        let mut expanded = true;
        let mut button = Rect::NOTHING;
        let mut frame = |events: Vec<egui::Event>, expanded: &mut bool| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), panel)),
                events,
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| {
                button = show_toolbar_toggle(ui, panel, expanded);
            });
            button
        };
        frame(vec![], &mut expanded);
        let original = frame(vec![], &mut expanded);
        let pos = original.center();
        for expected in [false, true, false, true] {
            frame(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                &mut expanded,
            );
            let rect = frame(
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                &mut expanded,
            );
            assert_eq!(expanded, expected);
            assert_eq!(rect, original);
        }
        assert_eq!(original.right(), panel.x - 8.0);
        assert_eq!(original.top(), 8.0);
    }
}

impl WheelHandler for AnnotatorState {
    fn on_scroll_delta_changed(&mut self, value: f32) {
        if value > 0. {
            let zoom = self.extra_zoom_factor - 0.1;
            if zoom >= 0.1 {
                self.extra_zoom_factor = zoom;
            }
        } else {
            let zoom = self.extra_zoom_factor + 0.1;
            self.extra_zoom_factor = zoom;
        }
    }
}
