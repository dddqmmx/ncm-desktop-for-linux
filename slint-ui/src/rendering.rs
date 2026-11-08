//! Keep the application and real-window checks on the same rendering path.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Renderer {
    OpenGl,
    Software,
    #[default]
    #[serde(other)]
    Vulkan,
}

impl Renderer {
    pub fn from_name(name: &str) -> Self {
        match name {
            "opengl" => Self::OpenGl,
            "software" => Self::Software,
            _ => Self::Vulkan,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Vulkan => "vulkan",
            Self::OpenGl => "opengl",
            Self::Software => "software",
        }
    }

    pub fn backend_selector(self) -> slint::BackendSelector {
        let selector = slint::BackendSelector::new().backend_name("winit".into());
        match self {
            // Select both renderer and API so environment defaults cannot turn
            // a saved software choice back into GPU rendering.
            Self::Vulkan => selector.renderer_name("skia".into()).require_vulkan(),
            Self::OpenGl => selector.renderer_name("femtovg".into()).require_opengl(),
            Self::Software => selector.renderer_name("software".into()),
        }
    }
}

// Real-window checks use a dedicated override, never the user's saved settings.
#[cfg(test)]
pub fn test_renderer() -> Renderer {
    std::env::var("NCM_SLINT_TEST_RENDERER")
        .map(|name| Renderer::from_name(&name))
        .unwrap_or_default()
}

#[cfg(test)]
pub fn backend_selector() -> slint::BackendSelector {
    test_renderer().backend_selector()
}
