const POINT_SCALE: usize = 18;
const POINT_FACTOR: u128 = 1_000_000_000_000_000_000;

#[derive(Default)]
pub(super) struct Total(u128);

impl Total {
    pub(super) fn add(&mut self, value: &str) -> Option<()> {
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        if whole.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || fraction.len() > POINT_SCALE
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let whole = whole.parse::<u128>().ok()?.checked_mul(POINT_FACTOR)?;
        let fraction = format!("{fraction:0<POINT_SCALE$}").parse::<u128>().ok()?;
        self.0 = self.0.checked_add(whole.checked_add(fraction)?)?;
        Some(())
    }

    pub(super) fn value(&self) -> String {
        let whole = self.0 / POINT_FACTOR;
        let fraction = format!("{:018}", self.0 % POINT_FACTOR);
        let fraction = fraction.trim_end_matches('0');
        if fraction.is_empty() {
            whole.to_string()
        } else {
            format!("{whole}.{fraction}")
        }
    }
}
