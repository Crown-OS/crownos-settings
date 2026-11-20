/// How a monitor is oriented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transform {
    #[default]
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

impl Transform {
    /// The four the page offers. The flipped variants exist in the protocol
    /// but describe mirrored panels, which is not something a user picks.
    pub const CHOICES: [Self; 4] = [
        Self::Normal,
        Self::Rotate90,
        Self::Rotate180,
        Self::Rotate270,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Landscape",
            Self::Rotate90 => "Portrait",
            Self::Rotate180 => "Landscape (flipped)",
            Self::Rotate270 => "Portrait (flipped)",
            Self::Flipped => "Mirrored",
            Self::Flipped90 => "Mirrored portrait",
            Self::Flipped180 => "Mirrored landscape",
            Self::Flipped270 => "Mirrored portrait (flipped)",
        }
    }

    /// Whether this orientation swaps width and height.
    pub fn is_upright(self) -> bool {
        matches!(
            self,
            Self::Rotate90 | Self::Rotate270 | Self::Flipped90 | Self::Flipped270
        )
    }
}

/// One mode a monitor can run at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mode {
    pub width: i32,
    pub height: i32,
    /// Millihertz, as the protocol counts it.
    pub refresh: i32,
    pub preferred: bool,
}

impl Mode {
    pub fn label(&self) -> String {
        format!(
            "{}×{} @ {:.2} Hz",
            self.width,
            self.height,
            f64::from(self.refresh) / 1000.0
        )
    }
}

/// One monitor, as the page shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// The connector, e.g. `"DP-1"`. Identifies it in a configuration.
    pub name: String,
    pub description: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub enabled: bool,
    pub modes: Vec<Mode>,
    pub current_mode: Option<usize>,
    pub position: (i32, i32),
    pub scale: f64,
    pub transform: Transform,
    /// Whether the compositor said anything about adaptive sync at all, which
    /// is what decides if the setting is offered.
    pub vrr_capable: bool,
    pub info: InfoState,
    /// `None` when the compositor does not serve brightness for this monitor.
    pub brightness: Option<Brightness>,
}

impl Monitor {
    /// What to call this monitor in the interface.
    ///
    /// The make and model where the panel provided them, because "Dell U2720Q"
    /// means something to a user and "DP-1" does not. The connector is kept
    /// alongside, since it is what distinguishes two identical monitors.
    pub fn title(&self) -> String {
        match (&self.make, &self.model) {
            (Some(make), Some(model)) => format!("{make} {model} ({})", self.name),
            (Some(name), None) | (None, Some(name)) => format!("{name} ({})", self.name),
            (None, None) => self.name.clone(),
        }
    }

    /// This monitor as a draft, i.e. "leave it exactly as it is".
    pub fn to_draft(&self) -> MonitorDraft {
        MonitorDraft {
            name: self.name.clone(),
            enabled: self.enabled,
            mode: self.current_mode,
            position: self.position,
            scale: self.scale,
            transform: self.transform,
        }
    }

    /// The desktop area it covers now, in logical pixels; zero without a mode.
    pub fn logical_size(&self) -> (i32, i32) {
        let mode = self.current_mode.and_then(|index| self.modes.get(index));
        logical_size(mode, self.scale, self.transform)
    }

    pub fn described(&self) -> Option<&OutputInfo> {
        match &self.info {
            InfoState::Described(info) => Some(info),
            InfoState::Unserved | InfoState::Waiting => None,
        }
    }

    /// The name `compositor.ron` files this monitor under after its connector.
    /// `None` while the compositor serves output info but has not described
    /// this monitor: a connector-named entry made then would hide the entry
    /// named by its identity.
    pub fn identity(&self) -> Option<&str> {
        match &self.info {
            InfoState::Described(info) => Some(&info.identity),
            InfoState::Unserved => Some(&self.name),
            InfoState::Waiting => None,
        }
    }

    /// A remote screen or a headless target, with no display behind it.
    pub fn is_virtual(&self) -> bool {
        self.described().map_or_else(
            || self.name.starts_with(VIRTUAL_CONNECTOR_PREFIX),
            |info| info.virtual_output,
        )
    }

    /// Whether `name`, as `compositor.ron` writes it, names this monitor.
    pub fn is_named(&self, name: &str) -> bool {
        self.name == name || self.identity() == Some(name)
    }
}

const VIRTUAL_CONNECTOR_PREFIX: &str = "VIRTUAL-";

/// What `crownos_output_info_v1` says about one monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfoState {
    /// The compositor does not serve output info.
    Unserved,
    /// The compositor serves it, but has not described this monitor (yet).
    Waiting,
    Described(OutputInfo),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutputInfo {
    /// Taken verbatim from the compositor, never rebuilt here.
    pub identity: String,
    pub primary: bool,
    pub hdr_capable: bool,
    pub virtual_output: bool,
    pub scanout_bits: u32,
}

/// How the compositor changes a monitor's brightness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrightnessControl {
    Fixed,
    Backlight,
    Ddc,
    Software,
}

impl BrightnessControl {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Not adjustable",
            Self::Backlight => "Backlight",
            Self::Ddc => "DDC/CI",
            Self::Software => "Software dimming",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Brightness {
    pub control: BrightnessControl,
    /// Perceptual, from 0.0 to 1.0.
    pub level: f32,
}

/// A logical rectangle: where a monitor starts, and how big it is.
pub type Rect = ((i32, i32), (i32, i32));

/// What a mode occupies once scale and orientation are applied.
pub fn logical_size(mode: Option<&Mode>, scale: f64, transform: Transform) -> (i32, i32) {
    let Some(mode) = mode else {
        return (0, 0);
    };
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let (width, height) = (
        (f64::from(mode.width) / scale).round() as i32,
        (f64::from(mode.height) / scale).round() as i32,
    );

    if transform.is_upright() {
        (height, width)
    } else {
        (width, height)
    }
}

/// The smallest box around every rectangle with an area, as its top-left and
/// bottom-right corners.
pub fn bounds(rects: &[Rect]) -> Option<((i32, i32), (i32, i32))> {
    rects
        .iter()
        .filter(|(_, (width, height))| *width > 0 && *height > 0)
        .map(|((x, y), (width, height))| ((*x, *y), (x + width, y + height)))
        .reduce(|(low, high), (other_low, other_high)| {
            (
                (low.0.min(other_low.0), low.1.min(other_low.1)),
                (high.0.max(other_high.0), high.1.max(other_high.1)),
            )
        })
}

/// Right of every rectangle in `rects`, level with the highest one.
pub fn beside(rects: &[Rect]) -> (i32, i32) {
    bounds(rects).map_or((0, 0), |((_, top), (right, _))| (right, top))
}

pub fn intersects(left: Rect, right: Rect) -> bool {
    let (((lx, ly), (lw, lh)), ((rx, ry), (rw, rh))) = (left, right);
    lx < rx + rw && rx < lx + lw && ly < ry + rh && ry < ly + lh
}

/// What the page wants one monitor to become.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorDraft {
    pub name: String,
    pub enabled: bool,
    pub mode: Option<usize>,
    pub position: (i32, i32),
    pub scale: f64,
    pub transform: Transform,
}

impl MonitorDraft {
    /// Where this draft would put the monitor, and how much of the desktop it
    /// would cover. The mode list is the monitor's, so it has to be given.
    pub fn geometry(&self, monitor: &Monitor) -> Rect {
        let mode = self.mode.and_then(|index| monitor.modes.get(index));
        (
            self.position,
            logical_size(mode, self.scale, self.transform),
        )
    }
}
