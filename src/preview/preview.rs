
use {
    crate::{
        command::{ScrollCommand},
        display::{Screen, W},
        errors::ProgramError,
        hex::HexView,
        pattern::Pattern,
        skin::PanelSkin,
        syntactic::SyntacticView,
    },
    std::path::{Path},
    termimad::{Area},
};

pub enum Preview {
    Syntactic(SyntacticView),
    Hex(HexView),
    IOError,
}

impl Preview {
    pub fn new(path: &Path, pattern: Pattern) -> Self {
        if let Ok(view) = time!(Debug, "new syntactic_view", SyntacticView::new(path, pattern)) {
            return Self::Syntactic(view);
        }
        match HexView::new(path.to_path_buf()) {
            Ok(reader) => Self::Hex(reader),
            Err(e) => {
                warn!("error while previewing {:?} : {:?}", path, e);
                Self::IOError
            }
        }
    }
    pub fn try_scroll(
        &mut self,
        cmd: ScrollCommand,
    ) -> bool {
        match self {
            Self::Syntactic(sv) => sv.try_scroll(cmd),
            Self::Hex(hv) => hv.try_scroll(cmd),
            _ => false,
        }
    }
    pub fn is_filterable(&self) -> bool {
        match self {
            Self::Syntactic(_) => true,
            _ => false,
        }
    }

    pub fn display(
        &mut self,
        w: &mut W,
        screen: &Screen,
        panel_skin: &PanelSkin,
        area: &Area,
    ) -> Result<(), ProgramError> {
        match self {
            Self::Syntactic(sv) => sv.display(w, screen, panel_skin, area),
            Self::Hex(hv) => hv.display(w, screen, panel_skin, area),
            Self::IOError => {
                debug!("nothing to display: IOError");
                // FIXME clear area
                Ok(())
            }
        }
    }
}
