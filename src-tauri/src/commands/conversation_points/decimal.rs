const POINT_SCALE: usize = 18;
const DECIMAL_RADIX: u8 = 10;

#[derive(Default)]
pub(super) struct Total(Vec<u8>);

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
        // 集合总额可以超过单行 DECIMAL(38,18) 的上限，不能使用固定宽度整数。
        let scaled = format!("{whole}{fraction:0<POINT_SCALE$}");
        self.0.resize(self.0.len().max(scaled.len()), 0);
        let mut carry = 0;
        for (index, digit) in scaled.bytes().rev().map(|byte| byte - b'0').enumerate() {
            let sum = self.0[index] + digit + carry;
            self.0[index] = sum % DECIMAL_RADIX;
            carry = sum / DECIMAL_RADIX;
        }
        let mut index = scaled.len();
        while carry > 0 {
            if index == self.0.len() {
                self.0.push(0);
            }
            let sum = self.0[index] + carry;
            self.0[index] = sum % DECIMAL_RADIX;
            carry = sum / DECIMAL_RADIX;
            index += 1;
        }
        Some(())
    }

    pub(super) fn value(&self) -> String {
        let digits: String = self
            .0
            .iter()
            .rev()
            .map(|digit| char::from(b'0' + digit))
            .collect();
        let digits = format!("{digits:0>width$}", width = POINT_SCALE + 1);
        let (whole, fraction) = digits.split_at(digits.len() - POINT_SCALE);
        let whole = whole.trim_start_matches('0');
        let whole = if whole.is_empty() { "0" } else { whole };
        let fraction = fraction.trim_end_matches('0');
        if fraction.is_empty() {
            whole.to_string()
        } else {
            format!("{whole}.{fraction}")
        }
    }
}
