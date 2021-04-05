use criterion::black_box;

#[derive(Copy, Clone)]
pub struct TickCounter(u64, f64);

impl TickCounter {
    pub fn new() -> TickCounter {
        let mut arr = [0; 10000];
        for _ in 1..1000 {
            let s = Self::raw_start();
            let e = Self::raw_end();
            black_box(e - s);
        }
        for v in arr.iter_mut() {
            let s = Self::raw_start();
            let e = Self::raw_end();
            *v = e - s;
        }
        arr.sort_unstable();
        for k in 0..1000 {
            arr[k] = arr[1000];
        }
        for k in 9000..10000 {
            arr[k] = arr[8999];
        }
        let s = arr.iter().fold(0, |cur, v| cur + *v);
        let zero = s / 10000;
        let mut k = 0;
        let mut arr = [0f64; 10000];
        loop {
            let s = std::time::Instant::now();
            let s0 = Self::raw_start();
            for _ in 1..1000 {
                let s = Self::raw_start();
                let e = Self::raw_end();
                black_box(e - s);
            }
            let e0 = Self::raw_end();
            let e = std::time::Instant::now();
            if s0 > e0 {
                continue;
            }
            let l = e.duration_since(s).as_nanos() as f64;
            let dst = (e0 - s0) as f64;
            arr[k] = l / dst;
            k += 1;
            if k == 10000 {
                break;
            }
        }
        arr.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for k in 0..1000 {
            arr[k] = arr[1000];
        }
        for k in 9000..10000 {
            arr[k] = arr[8999];
        }
        let s = arr.iter().fold(0f64, |cur, v| cur + *v);
        TickCounter(zero, s / 10000f64)
    }
    pub fn time<R, F: FnOnce() -> R>(&self, f: F) -> std::time::Duration {
        let s = Self::raw_start();
        black_box(f());
        let e = Self::raw_end();
        let v = (e - s) as f64;
        let v = (v - self.0 as f64) * self.1;
        let v = v.round();
        if v > 0f64 {
            std::time::Duration::from_nanos(v as u64)
        } else {
            std::time::Duration::from_nanos(0)
        }
    }
    #[inline(always)]
    fn raw_start() -> u64 {
        let high: u64;
        let low: u64;
        let cpuid_ask: u64 = 0;
        unsafe {
            asm!(
                 "cpuid",
                 "rdtsc",
                 out("rdx") high,
                 inout("rax") cpuid_ask => low,
                 out("rbx") _,
                 out("rcx") _,
                 options(nostack,preserves_flags)
            )
        };
        (high << 32) | low
    }
    #[inline(always)]
    fn raw_end() -> u64 {
        let high: u64;
        let low: u64;
        unsafe {
            asm!(
                 "rdtscp",
                 "mov {high}, rdx",
                 "mov {low}, rax",
                 "mov rax, 0",
                 "cpuid",
                 high = out(reg) high,
                 low = out(reg) low,
                 out("rax")  _,
                 out("rbx")  _,
                 out("rcx")  _,
                 out("rdx")  _,
                 options(nostack,preserves_flags)
            )
        };
        (high << 32) | low
    }
}
