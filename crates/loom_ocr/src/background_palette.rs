//! Bounded RGB modes, not a mean of unrelated background colors.

#[derive(Clone, Copy, Default)]
struct Bin {
    count: u32,
    sum: [u32; 3],
}

impl Bin {
    fn color(self) -> [u8; 3] {
        self.sum.map(|value| (value / self.count.max(1)) as u8)
    }
}

pub(super) struct Palette {
    bins: Vec<Bin>,
}

impl Palette {
    pub(super) fn new() -> Self {
        Self {
            bins: vec![Bin::default(); 4096],
        }
    }

    // Callers sample at most 128 * 128 pixels (or 512 perimeter pixels).
    pub(super) fn add(&mut self, color: [u8; 3]) {
        let index = ((color[0] as usize >> 4) << 8)
            | ((color[1] as usize >> 4) << 4)
            | (color[2] as usize >> 4);
        let bin = &mut self.bins[index];
        bin.count += 1;
        for (sum, channel) in bin.sum.iter_mut().zip(color) {
            *sum += u32::from(channel);
        }
    }

    pub(super) fn dominant(&self, foreground: Option<[u8; 3]>) -> Option<[u8; 3]> {
        self.select(|color| foreground.is_none_or(|text| distance(color, text) > 32))
    }

    pub(super) fn background(&self, ink: [u8; 3], reference: [u8; 3]) -> Option<[u8; 3]> {
        self.select(|color| !is_ink_blend(color, ink, reference))
    }

    fn select(&self, accept: impl Fn([u8; 3]) -> bool) -> Option<[u8; 3]> {
        let eligible = |bin: &Bin| bin.count > 0 && accept(bin.color());
        let mut best = Bin::default();
        // Adjacent bins are combined so compression noise straddling a quantization
        // boundary does not beat a genuinely dominant background. Fixed iteration
        // order resolves ties deterministically; no unbounded clustering loop.
        for (index, seed) in self
            .bins
            .iter()
            .enumerate()
            .filter(|(_, bin)| eligible(bin))
        {
            let channels = [index >> 8, (index >> 4) & 15, index & 15];
            let mut cluster = Bin::default();
            for red in channels[0].saturating_sub(1)..=(channels[0] + 1).min(15) {
                for green in channels[1].saturating_sub(1)..=(channels[1] + 1).min(15) {
                    for blue in channels[2].saturating_sub(1)..=(channels[2] + 1).min(15) {
                        let bin = self.bins[(red << 8) | (green << 4) | blue];
                        if !eligible(&bin) || distance(seed.color(), bin.color()) > 24 {
                            continue;
                        }
                        cluster.count += bin.count;
                        for (sum, value) in cluster.sum.iter_mut().zip(bin.sum) {
                            *sum += value;
                        }
                    }
                }
            }
            if cluster.count > best.count {
                best = cluster;
            }
        }
        (best.count > 0).then(|| best.color())
    }
}

// Antialiased edges lie between ink and substrate in RGB space. Exclude those
// blends as well as solid ink, but retain near-background noise (>= 85% substrate).
fn is_ink_blend(color: [u8; 3], ink: [u8; 3], background: [u8; 3]) -> bool {
    if distance(color, ink) <= 32 {
        return true;
    }
    let axis = (0..3)
        .max_by_key(|&axis| ink[axis].abs_diff(background[axis]))
        .unwrap();
    let span = f32::from(background[axis]) - f32::from(ink[axis]);
    if span.abs() < 32.0 {
        return false;
    }
    let alpha = (f32::from(color[axis]) - f32::from(ink[axis])) / span;
    (0.0..0.85).contains(&alpha)
        && (0..3).all(|channel| {
            let expected = f32::from(ink[channel])
                + alpha * (f32::from(background[channel]) - f32::from(ink[channel]));
            (f32::from(color[channel]) - expected).abs() <= 24.0
        })
}

pub(super) fn distance(left: [u8; 3], right: [u8; 3]) -> u8 {
    left.into_iter()
        .zip(right)
        .map(|(a, b)| a.abs_diff(b))
        .max()
        .unwrap_or(0)
}
