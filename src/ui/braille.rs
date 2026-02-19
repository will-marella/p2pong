use ratatui::style::Color;

/// Braille canvas for high-resolution terminal rendering
/// Each terminal cell contains a 2×4 grid of Braille dots
/// This gives us 2× horizontal and 4× vertical resolution

pub struct BrailleCanvas {
    width: usize,                    // Width in terminal cells
    height: usize,                   // Height in terminal cells
    dots: Vec<Vec<u8>>,              // 2D array of dot patterns (0-255)
    colors: Vec<Vec<Option<Color>>>, // 2D array of colors per cell
}

impl BrailleCanvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            dots: vec![vec![0; width]; height],
            colors: vec![vec![None; width]; height],
        }
    }

    /// Clear all dots and colors
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        for row in &mut self.dots {
            for cell in row {
                *cell = 0;
            }
        }
        for row in &mut self.colors {
            for cell in row {
                *cell = None;
            }
        }
    }

    /// Set a dot at pixel coordinates
    /// pixel_x: 0 to (width * 2 - 1)
    /// pixel_y: 0 to (height * 4 - 1)
    pub fn set_pixel(&mut self, pixel_x: usize, pixel_y: usize) {
        self.set_pixel_with_color(pixel_x, pixel_y, None);
    }

    /// Set a dot at pixel coordinates with a specific color
    /// pixel_x: 0 to (width * 2 - 1)
    /// pixel_y: 0 to (height * 4 - 1)
    pub fn set_pixel_with_color(&mut self, pixel_x: usize, pixel_y: usize, color: Option<Color>) {
        let cell_x = pixel_x / 2;
        let cell_y = pixel_y / 4;

        if cell_x >= self.width || cell_y >= self.height {
            return;
        }

        let dot_x = pixel_x % 2; // 0 or 1 (left or right column)
        let dot_y = pixel_y % 4; // 0, 1, 2, or 3 (row within cell)

        // Braille dot numbering:
        // 1 4
        // 2 5
        // 3 6
        // 7 8
        let dot_index = match (dot_x, dot_y) {
            (0, 0) => 0, // dot 1
            (0, 1) => 1, // dot 2
            (0, 2) => 2, // dot 3
            (0, 3) => 6, // dot 7
            (1, 0) => 3, // dot 4
            (1, 1) => 4, // dot 5
            (1, 2) => 5, // dot 6
            (1, 3) => 7, // dot 8
            _ => unreachable!(),
        };

        self.dots[cell_y][cell_x] |= 1 << dot_index;

        // Set color for this cell (if specified)
        if color.is_some() {
            self.colors[cell_y][cell_x] = color;
        }
    }

    /// Fill a rectangle with pixels
    pub fn fill_rect(&mut self, x: usize, y: usize, width: usize, height: usize) {
        self.fill_rect_with_color(x, y, width, height, None);
    }

    /// Fill a rectangle with pixels and a specific color
    pub fn fill_rect_with_color(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        color: Option<Color>,
    ) {
        for py in y..(y + height) {
            for px in x..(x + width) {
                self.set_pixel_with_color(px, py, color);
            }
        }
    }

    /// Convert dot pattern to Braille character
    /// Braille Unicode: U+2800 + dot pattern
    pub fn to_char(&self, cell_x: usize, cell_y: usize) -> char {
        if cell_x >= self.width || cell_y >= self.height {
            return ' ';
        }

        let pattern = self.dots[cell_y][cell_x];
        char::from_u32(0x2800 + pattern as u32).unwrap_or(' ')
    }

    /// Get the color for a cell
    pub fn get_color(&self, cell_x: usize, cell_y: usize) -> Option<Color> {
        if cell_x >= self.width || cell_y >= self.height {
            return None;
        }
        self.colors[cell_y][cell_x]
    }

    /// Get width in pixels (2 per cell)
    pub fn pixel_width(&self) -> usize {
        self.width * 2
    }

    /// Get height in pixels (4 per cell)
    pub fn pixel_height(&self) -> usize {
        self.height * 4
    }

    /// Draw a horizontal line (1 pixel thick) across the canvas
    pub fn draw_horizontal_line(&mut self, y: usize) {
        let width = self.pixel_width();
        for x in 0..width {
            self.set_pixel(x, y);
        }
    }

    /// Draw simple text using small 3x5 pixel characters
    /// Returns the width in pixels used
    pub fn draw_text(&mut self, text: &str, x: usize, y: usize) -> usize {
        let mut offset_x = 0;
        for ch in text.chars() {
            if ch == ' ' {
                offset_x += 2; // Space width
                continue;
            }
            let char_width = self.draw_small_char(ch, x + offset_x, y);
            offset_x += char_width + 1; // Character width + 1px spacing
        }
        offset_x
    }

    /// Draw a small 3x5 character (uppercase letters, simple blocky style)
    fn draw_small_char(&mut self, ch: char, x: usize, y: usize) -> usize {
        let pattern: &[u8] = match ch.to_ascii_uppercase() {
            'W' => &[
                0b101, 0b101, 0b101, 0b101, 0b010, // W
            ],
            'S' => &[
                0b111, 0b100, 0b111, 0b001, 0b111, // S
            ],
            'Q' => &[
                0b111, 0b101, 0b101, 0b111, 0b001, // Q
            ],
            'U' => &[
                0b101, 0b101, 0b101, 0b101, 0b111, // U
            ],
            'P' => &[
                0b111, 0b101, 0b111, 0b100, 0b100, // P
            ],
            'D' => &[
                0b110, 0b101, 0b101, 0b101, 0b110, // D
            ],
            'O' => &[
                0b111, 0b101, 0b101, 0b101, 0b111, // O
            ],
            'N' => &[
                0b101, 0b111, 0b111, 0b111, 0b101, // N
            ],
            'I' => &[
                0b1, 0b1, 0b1, 0b1, 0b1, // I (1 pixel wide)
            ],
            'T' => &[
                0b111, 0b010, 0b010, 0b010, 0b010, // T
            ],
            ':' => &[
                0b0, 0b1, 0b0, 0b1, 0b0, // :
            ],
            _ => return 3, // Default width for unknown chars
        };

        let width = if ch == 'I' || ch == ':' { 1 } else { 3 };

        for (row, bits) in pattern.iter().enumerate() {
            for col in 0..width {
                if (bits >> (width - 1 - col)) & 1 == 1 {
                    self.set_pixel(x + col, y + row);
                }
            }
        }

        width
    }

    /// Draw a block-style digit (0-9) at the given pixel position
    /// Each digit is 10 pixels wide × 16 pixels tall (5×4 cells)
    pub fn draw_digit(&mut self, digit: u8, x: usize, y: usize) {
        if digit > 9 {
            return;
        }

        // Block-style digit patterns (10×16 pixels)
        // Using classic 7-segment display inspired shapes
        let patterns: [[u16; 16]; 10] = [
            // 0
            [
                0b11111111_11,
                0b11111111_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11111111_11,
                0b11111111_11,
            ],
            // 1
            [
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
                0b00000011_11,
            ],
            // 2
            [
                0b11111111_11,
                0b11111111_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b11111111_11,
                0b11111111_11,
                0b11000000_00,
                0b11000000_00,
                0b11000000_00,
                0b11000000_00,
                0b11000000_00,
                0b11111111_11,
                0b11111111_11,
            ],
            // 3
            [
                0b11111111_11,
                0b11111111_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b11111111_11,
                0b11111111_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b11111111_11,
                0b11111111_11,
            ],
            // 4
            [
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11111111_11,
                0b11111111_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
            ],
            // 5
            [
                0b11111111_11,
                0b11111111_11,
                0b11000000_00,
                0b11000000_00,
                0b11000000_00,
                0b11000000_00,
                0b11000000_00,
                0b11111111_11,
                0b11111111_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b11111111_11,
                0b11111111_11,
            ],
            // 6
            [
                0b11111111_11,
                0b11111111_11,
                0b11000000_00,
                0b11000000_00,
                0b11000000_00,
                0b11000000_00,
                0b11000000_00,
                0b11111111_11,
                0b11111111_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11111111_11,
                0b11111111_11,
            ],
            // 7
            [
                0b11111111_11,
                0b11111111_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
            ],
            // 8
            [
                0b11111111_11,
                0b11111111_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11111111_11,
                0b11111111_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11111111_11,
                0b11111111_11,
            ],
            // 9
            [
                0b11111111_11,
                0b11111111_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11000000_11,
                0b11111111_11,
                0b11111111_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b00000000_11,
                0b11111111_11,
                0b11111111_11,
            ],
        ];

        let pattern = &patterns[digit as usize];

        // Draw the digit pixel by pixel
        for row in 0..16 {
            let row_bits = pattern[row];
            for col in 0..10 {
                if (row_bits >> (9 - col)) & 1 == 1 {
                    self.set_pixel(x + col, y + row);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_braille_canvas() {
        let mut canvas = BrailleCanvas::new(2, 2);

        // Set a single pixel
        canvas.set_pixel(0, 0);
        assert_eq!(canvas.to_char(0, 0), '⠁'); // dot 1

        // Fill a rectangle
        canvas.clear();
        canvas.fill_rect(0, 0, 4, 4);
        // Should have all dots filled in 2×1 cells
    }
}
