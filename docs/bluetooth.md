# Bluetooth devices

Search for **bluetooth**, **bt**, or **蓝牙**, then select **Connect or disconnect Bluetooth devices**. The picker lists devices already paired with macOS, with audio devices (such as headphones and speakers) first, then other devices. Within each group, connected devices come first, followed by device name. Device categories come from macOS Bluetooth metadata. Type a device name to filter, use the arrow keys to select, and press **Enter** to connect a disconnected device or disconnect a connected one.

The list stays open and displays **Connecting…** or **Disconnecting…** while the request runs in the background. Requests are serialized; Enter does not start another request while a refresh or connection operation is pending. Success is determined by the device's actual connection state. If a cached row is outdated and the device already has the requested state, Winlane leaves it in that state.

- **Command + R** refreshes device names and connection states.
- **Esc** closes the picker. An operation already sent to macOS may still finish.
- Backspace only deletes text; an empty input stays in Bluetooth.
- Click **‹ Bluetooth** to return to the main search.
- Assign a direct shortcut in **Settings → Shortcuts → Command shortcuts**.

On first use, macOS asks for Bluetooth access. If access was denied, enable Winlane in **System Settings → Privacy & Security → Bluetooth**, then return to the command and refresh. When Bluetooth is off, turn it on in System Settings first. Winlane does not change the radio's power or pair new devices.

Device discovery and connection operations use macOS's native Bluetooth APIs; no extra command-line tools are required. Discovery runs when entering the command or refreshing, with previous results held in memory for a quick first frame. Device names and addresses are not written to a Winlane history file. The command shows the paired devices exposed by macOS's IOBluetooth API; battery levels and unpaired nearby devices are not included. A device must be powered on and reachable to connect; a rejected or unsuccessful operation leaves an error visible in the picker.
