/// 基于第一个字节，返回 UTF-8 编码的字节数，如果不是有效的 UTF-8 编码，则返回 None
fn utf8_seq_len(b: u8) -> Option<usize> {
    match b {
        0x00..=0x7F => Some(1),
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

pub struct Utf8StreamValidator {
    pending: [u8; 4],
    len: usize,
}

impl Utf8StreamValidator {
    pub fn new() -> Self {
        Self {
            pending: [0; 4],
            len: 0,
        }
    }
    pub fn feed(&mut self, mut s: &[u8]) -> bool {
        if self.len > 0 {
            // 先补齐上一轮留下的残序列
            let need = match utf8_seq_len(self.pending[0]) {
                Some(n) => n,
                None => return false,
            };
            let more = (need - self.len).min(s.len());
            self.pending[self.len..self.len + more].copy_from_slice(&s[..more]);
            self.len += more;
            s = &s[more..];
            if self.len < need {
                return true;
            } // 还没凑齐，等下一轮
            if str::from_utf8(&self.pending[..need]).is_err() {
                return false;
            }
            self.len = 0;
        }
        match str::from_utf8(s) {
            Ok(_) => true,
            Err(e) if e.error_len().is_none() => {
                // 只是「意外结束」，把尾巴带走
                let tail = &s[e.valid_up_to()..];
                self.pending[..tail.len()].copy_from_slice(tail);
                self.len = tail.len();
                true
            }
            Err(_) => false, // 真正的非法编码
        }
    }
    pub fn finished(&self) -> bool {
        self.len == 0
    }
}
