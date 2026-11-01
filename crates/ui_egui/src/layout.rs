//! The UI layout: the touch-first kiosk layout or the desktop layout.
//!
//! - [`UiLayout::Mobile`] is the desk's 800x480 touchscreen layout: the icon
//!   rail on the left and a sub-page tab bar along the bottom of pages with
//!   sub-pages. The web UI zooms it to the browser window.
//! - [`UiLayout::Desktop`] is for a resizable window with a mouse: a
//!   navigation sidebar with page names that folds sub-pages underneath their
//!   page, and pages that fill the rest of the window.
//!
//! The desk picks it with `--layout desktop|mobile` (`-d` / `--desktop-mode`
//! / `--desktop` mean desktop, see `cli::UiArgs`), the web UI with
//! `?layout=desktop|mobile` in its URL ([`UiLayout::from_query`], default
//! [`UiLayout::web_default`]).

use std::{fmt, str::FromStr};

/// See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum UiLayout {
    /// The kiosk / touch layout (icon rail, sub-page tab bars).
    #[default]
    Mobile,
    /// Navigation sidebar with folded sub-pages, pages fill the window.
    Desktop,
}

impl UiLayout {
    pub fn is_desktop(self) -> bool {
        self == UiLayout::Desktop
    }

    pub fn as_str(self) -> &'static str {
        match self {
            UiLayout::Mobile => "mobile",
            UiLayout::Desktop => "desktop",
        }
    }

    /// The layout a URL query asks for: the value of its `layout` parameter
    /// (`?layout=desktop`, `?a=b&layout=mobile`; a leading `?` is optional).
    /// `None` without that parameter or with a value that is not a layout.
    pub fn from_query(query: &str) -> Option<Self> {
        query
            .trim_start_matches('?')
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == "layout")
            .and_then(|(_, value)| value.parse().ok())
    }

    /// The web UI's layout without `?layout=`: desktop for a browser window
    /// at least [`Self::WEB_DESKTOP_MIN_WIDTH`] CSS pixels wide whose primary
    /// pointer is a mouse or trackpad, otherwise mobile (phones, and tablets
    /// even in landscape, since their primary pointer is a finger).
    pub fn web_default(window_width_css: f64, coarse_pointer: bool) -> Self {
        if window_width_css >= Self::WEB_DESKTOP_MIN_WIDTH && !coarse_pointer {
            UiLayout::Desktop
        } else {
            UiLayout::Mobile
        }
    }

    /// See [`Self::web_default`].
    pub const WEB_DESKTOP_MIN_WIDTH: f64 = 1024.0;
}

impl FromStr for UiLayout {
    type Err = String;

    /// `desktop` or `mobile`, ignoring case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "desktop" => Ok(UiLayout::Desktop),
            "mobile" => Ok(UiLayout::Mobile),
            other => Err(format!(
                "unknown layout {other:?}, expected desktop or mobile"
            )),
        }
    }
}

impl fmt::Display for UiLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::UiLayout;

    #[test]
    fn parses_layout_names() {
        assert_eq!("desktop".parse(), Ok(UiLayout::Desktop));
        assert_eq!("Mobile".parse(), Ok(UiLayout::Mobile));
        assert!("kiosk".parse::<UiLayout>().is_err());
        assert_eq!(UiLayout::default(), UiLayout::Mobile);
    }

    #[test]
    fn reads_the_layout_query_parameter() {
        assert_eq!(UiLayout::from_query("?layout=desktop"), Some(UiLayout::Desktop));
        assert_eq!(UiLayout::from_query("layout=mobile"), Some(UiLayout::Mobile));
        assert_eq!(
            UiLayout::from_query("?debug=1&layout=desktop"),
            Some(UiLayout::Desktop)
        );
        assert_eq!(UiLayout::from_query(""), None);
        assert_eq!(UiLayout::from_query("?layouts=desktop"), None);
        assert_eq!(UiLayout::from_query("?layout=tv"), None);
    }

    #[test]
    fn web_default_follows_width_and_pointer() {
        assert_eq!(UiLayout::web_default(1440.0, false), UiLayout::Desktop);
        assert_eq!(UiLayout::web_default(1024.0, false), UiLayout::Desktop);
        assert_eq!(UiLayout::web_default(1023.0, false), UiLayout::Mobile);
        // A tablet in landscape: wide, but touch.
        assert_eq!(UiLayout::web_default(1180.0, true), UiLayout::Mobile);
        assert_eq!(UiLayout::web_default(390.0, true), UiLayout::Mobile);
    }
}
