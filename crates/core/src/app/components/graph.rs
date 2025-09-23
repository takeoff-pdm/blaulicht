//
// Graph component
//

use egui::{Color32, Context};

const CAP: usize = 1000;
const TIME_WINDOW_MS: u64 = 3000;
const GRAPH_UPDATE_INTERVAL_MS: u64 = 5;

/// Modular time series graph component
pub struct TimeSeriesGraph {
    time_series_data: Vec<(u64, i32)>,
    start_time: u64,
    last_update: u64,
    title: String,
    min_value: i32,
    max_value: i32,
    line_color: egui::Color32,
    shadow_color: egui::Color32,
}

impl TimeSeriesGraph {
    pub fn new(title: String, min_value: i32, max_value: i32, line_color: egui::Color32) -> Self {
        let start_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            time_series_data: {
                let mut v = vec![];
                for i in 0..100 {
                    v.push((i * 20, 0))
                }
                v
            },
            start_time,
            last_update: 0,
            title,
            min_value,
            max_value,
            line_color,
            shadow_color: egui::Color32::from_gray(40),
        }
    }

    /// Add a new data point to the time series
    pub fn add_data_point(&mut self, value: i32) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let relative_time = current_time - self.start_time;
        self.time_series_data.push((relative_time, value));
        if self.time_series_data.len() > CAP {
            self.time_series_data.remove(0);
        }
    }

    /// Update the graph with new data if enough time has passed
    pub fn update(&mut self, current_value: i32) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let elapsed = current_time - self.last_update;
        if elapsed >= GRAPH_UPDATE_INTERVAL_MS {
            self.add_data_point(current_value);
            self.last_update = current_time;
        }
    }

    /// Create smooth curve points using cubic interpolation
    fn create_smooth_curve(&self, points: &[egui::Pos2], y_offset: f32) -> Vec<egui::Pos2> {
        if points.len() < 2 {
            return points.to_vec();
        }

        let mut smooth_points = Vec::new();
        let segments_per_point = 8; // Number of interpolated points between each original point

        for i in 0..points.len() - 1 {
            let p0 = if i > 0 { points[i - 1] } else { points[i] };
            let p1 = points[i];
            let p2 = points[i + 1];
            let p3 = if i + 2 < points.len() {
                points[i + 2]
            } else {
                points[i + 1]
            };

            for j in 0..=segments_per_point {
                let t = j as f32 / segments_per_point as f32;
                let x = self.cubic_interpolate(p0.x, p1.x, p2.x, p3.x, t);
                let y = self.cubic_interpolate(p0.y, p1.y, p2.y, p3.y, t) + y_offset;
                smooth_points.push(egui::pos2(x, y));
            }
        }

        smooth_points
    }

    /// Cubic interpolation between four points
    fn cubic_interpolate(&self, p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
        let t2 = t * t;
        let t3 = t2 * t;

        // Catmull-Rom spline coefficients
        let a0 = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
        let a1 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
        let a2 = -0.5 * p0 + 0.5 * p2;
        let a3 = p1;

        a0 * t3 + a1 * t2 + a2 * t + a3
    }

    /// Draw the time series graph using a painter
    pub fn draw(&self, painter: egui::Painter, rect: egui::Rect) {
        if self.time_series_data.is_empty() {
            painter.text(
                egui::pos2(rect.min.x + 10.0, rect.center().y),
                egui::Align2::CENTER_CENTER,
                "No data available",
                egui::FontId::proportional(16.0),
                egui::Color32::from_gray(150),
            );
            return;
        }
        // Use real time for smooth scrolling, but convert to relative time for filtering
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let relative_now = now - self.start_time;
        let min_relative_time = relative_now.saturating_sub(TIME_WINDOW_MS);
        let actual_data: Vec<_> = self
            .time_series_data
            .iter()
            .filter(|(t, _)| *t >= min_relative_time)
            .collect();
        if actual_data.len() < 2 {
            painter.text(
                egui::pos2(rect.min.x + 10.0, rect.center().y),
                egui::Align2::CENTER_CENTER,
                "No data available",
                egui::FontId::proportional(16.0),
                egui::Color32::from_gray(150),
            );
            return;
        }
        let graph_rect = rect.shrink(15.0);

        // Professional dark background
        let bg_color = egui::Color32::from_rgb(20, 20, 25);
        painter.rect_filled(graph_rect, 0.0, bg_color);

        // Smooth scrolling: X position based on timestamp
        let value_range = (self.max_value - self.min_value).max(1);
        let mut points: Vec<egui::Pos2> = actual_data
            .iter()
            .map(|(t, value)| {
                let x_frac = (*t as f32 - min_relative_time as f32) / (TIME_WINDOW_MS as f32);
                let x = graph_rect.min.x + x_frac.clamp(0.0, 1.0) * graph_rect.width();
                let y = (graph_rect.max.y
                    - ((*value - self.min_value) as f32 / value_range as f32)
                        * graph_rect.height())
                .round();
                egui::pos2(x, y)
            })
            .collect();

        // Downsample points if there are more points than pixels available
        let max_points = (graph_rect.width() as usize).max(1);
        let mut downsampled_points = Vec::new();
        if points.len() > max_points {
            let bin_size = points.len() as f32 / max_points as f32;
            for i in 0..max_points {
                let start = (i as f32 * bin_size).floor() as usize;
                let end = ((i as f32 + 1.0) * bin_size).ceil() as usize;
                let end = end.min(points.len());
                if start < end {
                    // Average the points in this bin
                    let (sum_x, sum_y, count) =
                        points[start..end].iter().fold((0.0, 0.0, 0), |acc, p| {
                            (acc.0 + p.x, acc.1 + p.y, acc.2 + 1)
                        });
                    downsampled_points.push(egui::pos2(sum_x / count as f32, sum_y / count as f32));
                }
            }
            points = downsampled_points;
        }

        // Draw smooth curves between points using cubic interpolation
        if points.len() > 1 {
            // Remove shadow rendering
            // Create smooth curve points for main line
            let smooth_points = self.create_smooth_curve(&points, 0.0);
            for i in 0..smooth_points.len() - 1 {
                painter.line_segment(
                    [smooth_points[i], smooth_points[i + 1]],
                    (2.0, self.line_color),
                );
            }
        }
        // Title and current value
        let current_value = if let Some((_, value)) = actual_data.last() {
            *value
        } else {
            0
        };

        // Title with current value - styled with padding, smaller monospace font, and translucent background
        let title_text = format!("{}: {}", self.title, current_value);
        let title_font = egui::FontId::monospace(14.0);
        let padding = 8.0;
        let margin = 4.0;
        let estimated_width = title_text.len() as f32 * 8.0; // Rough estimate for monospace font
        let estimated_height = 20.0; // Fixed height for the background

        // Place background in the upper-left corner, fully inside the graph
        let bg_left = rect.min.x + margin;
        let bg_top = rect.min.y + margin;
        let bg_right = (bg_left + estimated_width + padding * 2.0).min(rect.max.x - margin);
        let bg_bottom = (bg_top + estimated_height + padding * 2.0).min(rect.max.y - margin);
        let bg_rect =
            egui::Rect::from_min_max(egui::pos2(bg_left, bg_top), egui::pos2(bg_right, bg_bottom));

        // Draw translucent background
        let bg_color = egui::Color32::from_rgba_premultiplied(0, 0, 0, 180); // Semi-transparent black
        painter.rect_filled(bg_rect, 4.0, bg_color);

        // Draw title text, always inside the background
        let text_x = bg_left + padding;
        let text_y = bg_top + padding;
        painter.text(
            egui::pos2(text_x, text_y),
            egui::Align2::LEFT_TOP,
            &title_text,
            title_font,
            egui::Color32::from_rgb(220, 220, 220),
        );
    }

    /// Clear all data points
    pub fn clear(&mut self) {
        self.time_series_data.clear();
    }

    /// Get the number of data points
    pub fn data_points_count(&self) -> usize {
        self.time_series_data.len()
    }
}
