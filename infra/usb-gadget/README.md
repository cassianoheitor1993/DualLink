# DualLink USB Gadget (CDC-NCM)

This directory contains scripts to configure a Linux machine as a USB network
device using the CDC-NCM (Network Control Model) standard. When connected to a
macOS host via USB-C, this creates a virtual Ethernet interface on both sides.

## How It Works

```
┌─────────────┐  USB-C  ┌──────────────┐
│ macOS (Host) │ ◄─────► │ Linux (Gadget)│
│  en<N>       │         │  usb0         │
│  10.0.1.2    │         │  10.0.1.1     │
└─────────────┘         └──────────────┘
```

The existing DualLink TCP/UDP transport works unchanged over this USB Ethernet
link. Benefits over Wi-Fi:
- **~1ms** transport latency vs ~5-10ms for Wi-Fi
- **Stable** — no interference, no packet loss
- **No router** needed

## Prerequisites

- Linux kernel 4.x+ with `configfs` and `libcomposite`
- USB-C port that supports USB Device Controller (UDC) / gadget mode
- `modprobe libcomposite usb_f_ncm`

### Checking UDC Support

```bash
ls /sys/class/udc/
```

If this directory is empty, your machine doesn't support USB gadget mode.
Most laptops with USB-C Type-C controllers (dwc3, cdns3) support it.

## Quick Start

```bash
# Setup (run on Linux receiver)
sudo ./setup-usb-gadget.sh

# Connect USB-C cable to Mac

# On Mac, configure the new network interface:
# System Preferences → Network → select USB Ethernet → Configure IPv4: Manually
# IP: 10.0.1.2, Subnet: 255.255.255.0

# Verify connectivity
ping 10.0.1.1    # from Mac
ping 10.0.1.2    # from Linux

# Start DualLink receiver as usual
cd /path/to/linux-receiver && cargo run

# On Mac app, the transport auto-detection will prefer USB over Wi-Fi
```

## Systemd Service (Auto-start)

```bash
sudo cp setup-usb-gadget.sh teardown-usb-gadget.sh /opt/duallink/
sudo chmod +x /opt/duallink/*.sh
sudo cp duallink-usb-gadget.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now duallink-usb-gadget
```

## Teardown

```bash
sudo ./teardown-usb-gadget.sh
# or
sudo systemctl stop duallink-usb-gadget
```

## Troubleshooting

| Issue | Check |
|-------|-------|
| No `/sys/class/udc/` entries | Kernel doesn't support gadget mode |
| `usb0` interface not appearing | Check `dmesg` for UDC errors |
| Mac doesn't see network interface | Try different USB-C port, check cable (must support data) |
| Can't ping peer | Verify IP configuration on both sides |
