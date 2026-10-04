use crate::grid::Grid;
use vte::Perform;

pub struct Terminal {
    pub grid: Grid,
}

impl Perform for Terminal {
    fn print(&mut self, c: char) {
        self.grid.put_char(c);
    }
    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => self.grid.carriage_return(),
            b'\n' => self.grid.newline(),
            _ => {}
        }
    }
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if action == 'H' {
            let mut iter = params.iter();
            let row = iter.next().and_then(|p| p.first()).copied().unwrap_or(1) as usize;
            let col = iter.next().and_then(|p| p.first()).copied().unwrap_or(1) as usize;
            self.grid.move_cursor(row, col);
        }
    }
}
