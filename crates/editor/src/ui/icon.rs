use egui::{Image, include_image};

#[allow(dead_code)]
pub enum Icons {
    DrawRectangle,
    DrawEllipse,
    DrawLine,
    DrawArrow,
    DrawFreehand,
    DrawHighlight,
    PixelArtTrace,
    BlurFx,
    DrawText,
    DrawNumber,
    DrawEraser,
    EditUndo,
    EditRedo,
    ZoomOriginal,
    DocumentSave,
    EditCopy,
    DialogOk,
    Sticky,
    CursorMove,
}

impl Icons {
    pub fn get_image(&self) -> Image<'_> {
        let image_source = match self {
            Icons::DrawRectangle => {
                include_image!("../../assets/icons/draw-rectangle.svg")
            }
            Icons::DrawEllipse => {
                include_image!("../../assets/icons/draw-ellipse.svg")
            }
            Icons::DrawLine => {
                include_image!("../../assets/icons/draw-line.svg")
            }
            Icons::DrawArrow => {
                include_image!("../../assets/icons/draw-arrow.svg")
            }
            Icons::DrawFreehand => {
                include_image!("../../assets/icons/draw-freehand.svg")
            }
            Icons::DrawHighlight => {
                include_image!("../../assets/icons/draw-highlight.svg")
            }
            Icons::PixelArtTrace => {
                include_image!("../../assets/icons/pixelart-trace.svg")
            }
            Icons::BlurFx => {
                include_image!("../../assets/icons/blurfx.svg")
            }
            Icons::DrawText => {
                include_image!("../../assets/icons/draw-text.svg")
            }
            Icons::DrawNumber => {
                include_image!("../../assets/icons/draw-number.svg")
            }
            Icons::DrawEraser => {
                include_image!("../../assets/icons/draw-eraser.svg")
            }
            Icons::EditUndo => {
                include_image!("../../assets/icons/edit-undo.svg")
            }
            Icons::EditRedo => {
                include_image!("../../assets/icons/edit-redo.svg")
            }
            Icons::ZoomOriginal => {
                include_image!("../../assets/icons/zoom-original.svg")
            }
            Icons::DocumentSave => {
                include_image!("../../assets/icons/document-save.svg")
            }
            Icons::EditCopy => {
                include_image!("../../assets/icons/edit-copy.svg")
            }
            Icons::DialogOk => {
                include_image!("../../assets/icons/dialog-ok.svg")
            }
            Icons::Sticky => {
                include_image!("../../assets/icons/sticky.svg")
            }
            Icons::CursorMove => {
                include_image!("../../assets/icons/cursor-move.svg")
            }
        };
        Image::new(image_source)
    }
}
