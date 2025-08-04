use std::collections::BTreeSet;

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct TagPool {
    free: BTreeSet<u16>,
    lowest: u16
}

#[allow(dead_code)]
impl TagPool {
    pub fn get(&mut self) -> Option<u16> {
        if let Some(v) = self.free.pop_first() {
            Some(v)
        } else {
            let v = self.lowest;
            self.lowest = self.lowest.checked_add(1)?;
            Some(v)
        }
    }

    pub fn put(&mut self, v: u16) {
        assert!(v < self.lowest);
        assert!(self.free.insert(v));
    }
}