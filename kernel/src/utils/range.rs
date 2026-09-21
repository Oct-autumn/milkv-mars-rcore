#[derive(Clone, Copy)]
pub struct Range<T>
where
    T: PartialOrd + Ord + Copy + Clone,
{
    start: T,
    end: T,
    step_fn: fn(T) -> T,
}

impl<T> Range<T>
where
    T: PartialOrd + Ord + Copy + Clone,
{
    /// 创建一个新的累加范围
    ///
    /// ## 参数
    /// - `start` 起始值
    /// - `end` 结束值
    /// - `step_fn` 步长函数，用于计算下一个值
    pub fn new(start: T, end: T, step_fn: fn(T) -> T) -> Self {
        assert!(start < end);
        assert!(step_fn(start) > start); // 防止 step 为负数或零，导致死循环
        Self {
            start,
            end,
            step_fn,
        }
    }

    pub fn start(&self) -> T {
        self.start
    }

    pub fn end(&self) -> T {
        self.end
    }
}

#[derive(Clone, Copy)]
pub struct RangeIterator<T>
where
    T: PartialOrd + Ord + Copy + Clone,
{
    range: Range<T>,
    current: T,
}

impl<T> Iterator for RangeIterator<T>
where
    T: PartialOrd + Ord + Copy + Clone,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.range.end {
            let result = self.current;
            self.current = (self.range.step_fn)(self.current);
            Some(result)
        } else {
            None
        }
    }
}

impl<T> IntoIterator for Range<T>
where
    T: PartialOrd + Ord + Copy + Clone,
{
    type Item = T;
    type IntoIter = RangeIterator<T>;

    fn into_iter(self) -> Self::IntoIter {
        RangeIterator {
            range: self,
            current: self.start,
        }
    }
}
