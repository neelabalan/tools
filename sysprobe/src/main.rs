use serde::Serialize;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind, System};

const KB: f64 = 1024.0;
const MB: f64 = KB * 1024.0;
const GB: f64 = MB * 1024.0;

#[allow(clippy::cast_precision_loss)]
fn format_bytes(bytes: u64) -> String {
    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.2} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.2} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.2} KB", bytes_f / KB)
    } else {
        format!("{bytes} B")
    }
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m {secs}s")
    } else if hours > 0 {
        format!("{hours}h {minutes}m {secs}s")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

fn format_timestamp(timestamp: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp);
    let secs = datetime.duration_since(UNIX_EPOCH).unwrap().as_secs();

    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_ymd(days_since_epoch);

    format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02} UTC")
}

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let mut remaining = days as i64;
    let mut year = 1970i64;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let days_in_months: [i64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u64;
    for days_in_month in &days_in_months {
        if remaining < *days_in_month {
            break;
        }
        remaining -= days_in_month;
        month += 1;
    }

    (year as u64, month, remaining as u64 + 1)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[derive(Serialize)]
struct CpuInfo {
    brand: String,
    vendor_id: String,
    physical_core_count: Option<usize>,
    logical_core_count: usize,
    frequency_mhz: u64,
}

#[derive(Serialize)]
struct MemoryInfo {
    total: String,
    used: String,
    free: String,
    available: String,
    total_swap: String,
    used_swap: String,
}

#[derive(Serialize)]
struct OsInfo {
    name: Option<String>,
    version: Option<String>,
    kernel_version: Option<String>,
    os_long_version: Option<String>,
    distribution_id: String,
    hostname: Option<String>,
}

#[derive(Serialize)]
struct DiskInfo {
    name: String,
    mount_point: String,
    file_system: String,
    kind: String,
    total: String,
    available: String,
    used: String,
    is_removable: bool,
}

#[derive(Serialize)]
struct NetworkInterface {
    name: String,
    mac_address: String,
    received: String,
    transmitted: String,
    received_packets: u64,
    transmitted_packets: u64,
    received_errors: u64,
    transmitted_errors: u64,
}

#[derive(Serialize)]
struct GpuInfo {
    name: String,
    vendor: String,
    device_type: String,
    backend: String,
    driver: String,
    driver_info: String,
}

#[derive(Serialize)]
struct SystemInfo {
    cpu: CpuInfo,
    memory: MemoryInfo,
    os: OsInfo,
    gpu: Vec<GpuInfo>,
    disks: Vec<DiskInfo>,
    networks: Vec<NetworkInterface>,
    boot_time: String,
    uptime: String,
}

fn gather_cpu_info(sys: &System) -> CpuInfo {
    let cpus = sys.cpus();
    let first_cpu = cpus.first();

    CpuInfo {
        brand: first_cpu.map(|c| c.brand().to_string()).unwrap_or_default(),
        vendor_id: first_cpu
            .map(|c| c.vendor_id().to_string())
            .unwrap_or_default(),
        physical_core_count: sys.physical_core_count(),
        logical_core_count: cpus.len(),
        frequency_mhz: first_cpu.map_or(0, sysinfo::Cpu::frequency),
    }
}

fn gather_memory_info(sys: &System) -> MemoryInfo {
    MemoryInfo {
        total: format_bytes(sys.total_memory()),
        used: format_bytes(sys.used_memory()),
        free: format_bytes(sys.free_memory()),
        available: format_bytes(sys.available_memory()),
        total_swap: format_bytes(sys.total_swap()),
        used_swap: format_bytes(sys.used_swap()),
    }
}

fn gather_os_info() -> OsInfo {
    OsInfo {
        name: System::name(),
        version: System::os_version(),
        kernel_version: System::kernel_version(),
        os_long_version: System::long_os_version(),
        distribution_id: System::distribution_id(),
        hostname: System::host_name(),
    }
}

fn gather_disk_info() -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .map(|disk| {
            let total = disk.total_space();
            let available = disk.available_space();
            let used = total.saturating_sub(available);
            DiskInfo {
                name: disk.name().to_string_lossy().to_string(),
                mount_point: disk.mount_point().to_string_lossy().to_string(),
                file_system: disk.file_system().to_string_lossy().to_string(),
                kind: format!("{:?}", disk.kind()),
                total: format_bytes(total),
                available: format_bytes(available),
                used: format_bytes(used),
                is_removable: disk.is_removable(),
            }
        })
        .collect()
}

fn gather_network_info() -> Vec<NetworkInterface> {
    let networks = Networks::new_with_refreshed_list();
    networks
        .iter()
        .map(|(name, data)| NetworkInterface {
            name: name.clone(),
            mac_address: data.mac_address().to_string(),
            received: format_bytes(data.total_received()),
            transmitted: format_bytes(data.total_transmitted()),
            received_packets: data.total_packets_received(),
            transmitted_packets: data.total_packets_transmitted(),
            received_errors: data.total_errors_on_received(),
            transmitted_errors: data.total_errors_on_transmitted(),
        })
        .collect()
}

fn gather_gpu_info() -> Vec<GpuInfo> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let adapters = instance.enumerate_adapters(wgpu::Backends::all());

    adapters
        .into_iter()
        .map(|adapter: wgpu::Adapter| {
            let info = adapter.get_info();
            GpuInfo {
                name: info.name,
                vendor: format_vendor(info.vendor),
                device_type: format_device_type(info.device_type),
                backend: format!("{:?}", info.backend),
                driver: info.driver,
                driver_info: info.driver_info,
            }
        })
        .collect()
}

fn format_vendor(vendor: u32) -> String {
    match vendor {
        0x0000 | 0x106B => "Apple".to_string(),
        0x1002 => "AMD".to_string(),
        0x1010 => "ImgTec".to_string(),
        0x10DE => "NVIDIA".to_string(),
        0x13B5 => "ARM".to_string(),
        0x5143 => "Qualcomm".to_string(),
        0x8086 => "Intel".to_string(),
        _ => format!("Unknown (0x{vendor:04X})"),
    }
}

fn format_device_type(device_type: wgpu::DeviceType) -> String {
    match device_type {
        wgpu::DeviceType::IntegratedGpu => "Integrated GPU".to_string(),
        wgpu::DeviceType::DiscreteGpu => "Discrete GPU".to_string(),
        wgpu::DeviceType::VirtualGpu => "Virtual GPU".to_string(),
        wgpu::DeviceType::Cpu => "CPU (Software)".to_string(),
        wgpu::DeviceType::Other => "Other".to_string(),
    }
}

fn gather_system_info() -> SystemInfo {
    let sys = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything()),
    );

    SystemInfo {
        cpu: gather_cpu_info(&sys),
        memory: gather_memory_info(&sys),
        os: gather_os_info(),
        gpu: gather_gpu_info(),
        disks: gather_disk_info(),
        networks: gather_network_info(),
        boot_time: format_timestamp(System::boot_time()),
        uptime: format_uptime(System::uptime()),
    }
}

fn main() {
    let info = gather_system_info();
    let json = serde_json::to_string_pretty(&info).expect("Failed to serialize system info");
    println!("{json}");
}
