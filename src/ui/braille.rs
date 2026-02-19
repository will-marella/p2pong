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

        // Set a single pixel and verify the correct Braille character is produced
        canvas.set_pixel(0, 0);
        assert_eq!(canvas.to_char(0, 0), '⠁'); // dot 1
    }
}
