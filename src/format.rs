use crate::model::FileSize;

pub enum SizeBase {
    Base2,
    Base10,
}

impl SizeBase {
    fn units(&self) -> [&str; 5] {
        match self {
            SizeBase::Base2 => ["B", "KiB", "MiB", "GiB", "TiB"],
            SizeBase::Base10 => ["B", "KB", "MB", "GB", "TB"],
        }
    }

    const fn kilo(&self) -> f64 {
        match self {
            SizeBase::Base2 => 1024.0,
            SizeBase::Base10 => 1000.0,
        }
    }
}

pub fn format_disk_size(bytes: FileSize, base: SizeBase) -> String {
    let kib: f64 = base.kilo();
    let units: [&str; 5] = base.units();

    if (bytes as f64) < kib {
        return format!("{bytes} B");
    }

    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= kib && unit < units.len() - 1 {
        size /= kib;
        unit += 1;
    }

    let formatted = format!("{size:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();

    format!("{formatted} {}", units[unit])
}

pub fn print_format_table(lines: &[Vec<String>], gap: usize) {
    let mut maxes = vec![0; lines[0].len()];

    for line in lines {
        for (i, item) in line.iter().enumerate() {
            maxes[i] = maxes[i].max(item.len());
        }
    }

    for line in lines {
        for (i, item) in line.iter().enumerate() {
            print!("{:<width$}", item, width = maxes[i] + gap);
        }

        println!();
    }
}
