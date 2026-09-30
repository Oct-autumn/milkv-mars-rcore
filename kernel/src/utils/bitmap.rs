use alloc::vec::Vec;

/// 简易位图
pub struct BitMap {
    bits: Vec<u64>,
    size: usize,
}

impl BitMap {
    /// 创建一个新的位图
    pub fn new(size: usize) -> Self {
        let bits = vec![0; size.div_ceil(64)]; // 每 64 位为一个块，计算需要多少块
        Self { bits, size }
    }

    /// 设置指定索引的位为 1
    pub fn set(&mut self, index: usize) {
        assert!(index < self.size);
        let (block, offset) = (index / 64, index % 64);
        self.bits[block] |= 1 << offset;
    }

    /// 设置指定索引的位为 0
    pub fn clear(&mut self, index: usize) {
        assert!(index < self.size);
        let (block, offset) = (index / 64, index % 64);
        self.bits[block] &= !(1 << offset);
    }

    /// 查找第一个未被设置的位的索引，如果所有位都已设置，则返回 None
    pub fn find_first_unset(&self) -> Option<usize> {
        for (block_index, &block) in self.bits.iter().enumerate() {
            if block != u64::MAX {
                // 如果该块不是全 1
                for offset in 0..64 {
                    let index = block_index * 64 + offset;
                    if index >= self.size {
                        return None; // 超出位图大小
                    }
                    if (block & (1 << offset)) == 0 {
                        return Some(index);
                    }
                }
            }
        }
        None // 所有位都已设置
    }

    /// 重置位图，将所有位清零
    pub fn reset(&mut self) {
        self.bits.fill(0);
    }
}
