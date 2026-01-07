/// Braille canvas for high-resolution terminal rendering
/// Each terminal cell contains a 2×4 grid of Braille dots
/// This gives us 2× horizontal and 4× vertical resolution

pub struct BrailleCanvas {
    width: usize,  // Width in terminal cells
    height: usize, // Height in terminal cells
    dots: Vec<Vec<u8>>, // 2D array of dot patterns (0-255)
}

impl BrailleCanvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            dots: vec![vec![0; width]; height],
        }
    }

    /// Clear all dots
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        for row in &mut self.dots {
            for cell in row {
                *cell = 0;
            }
        }
    }

    /// Set a dot at pixel coordinates
    /// pixel_x: 0 to (width * 2 - 1)
    /// pixel_y: 0 to (height * 4 - 1)
    pub fn set_pixel(&mut self, pixel_x: usize, pixel_y: usize) {
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
    }

    /// Fill a rectangle with pixels
    pub fn fill_rect(&mut self, x: usize, y: usize, width: usize, height: usize) {
        for py in y..(y + height) {
            for px in x..(x + width) {
                self.set_pixel(px, py);
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

    /// Get the canvas as a string (for rendering)
    #[allow(dead_code)]
    pub fn to_string(&self) -> String {
        let mut result = String::new();
        for y in 0..self.height {
            for x in 0..self.width {
                result.push(self.to_char(x, y));
            }
            if y < self.height - 1 {
                result.push('\n');
            }
        }
        result
    }

    /// Get width in pixels (2 per cell)
    pub fn pixel_width(&self) -> usize {
        self.width * 2
    }

    /// Get height in pixels (4 per cell)
    pub fn pixel_height(&self) -> usize {
        self.height * 4
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
