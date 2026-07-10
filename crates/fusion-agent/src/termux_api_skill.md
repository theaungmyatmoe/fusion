---
name: termux-api
description: |
  Control Android devices via Termux API. Auto-detects direct access (Termux/proot-distro) or falls back to SSH. Access camera, sensors, location, notifications, SMS, clipboard, TTS, and 70+ other device APIs.
---

# Termux API Skill

Control Android devices via Termux API commands. Detects if running inside Termux/proot-distro for direct access, or uses SSH for remote control.

## Prerequisites

### Para cualquier escenario
- Termux app instalada desde F-Droid o GitHub Releases
- Termux:API app instalada y permisos concedidos (Android Settings → Apps → Termux:API → Permissions)
- `termux-api` package: `pkg install termux-api`

### Native Termux (openCode corriendo directo en Termux)
- openCode compilado para aarch64 (ej: `guysoft/opencode-termux`)
- Los binarios `termux-*` ya están en `$PREFIX/bin` (en PATH por defecto)
- `termux-api-start` para iniciar el servicio si es necesario

### proot-distro (openCode desde Ubuntu/Debian/Arch via proot)
- Montar `/data/data/com.termux/files` dentro del proot (viene por defecto)
- Acceso a binarios via ruta completa: `/data/data/com.termux/files/usr/bin/termux-*`
- O agregar al PATH: `export PATH=$PATH:/data/data/com.termux/files/usr/bin`

### Remoto (SSH desde otro dispositivo)
- SSH configurado en Termux: `pkg install openssh && sshd`
- Puerto por defecto: 8022
- Autenticación: `passwd` o llaves SSH en `~/.ssh/authorized_keys`

## Environment Detection

Before running commands, check if direct access is available:

```bash
# Detect environment: native Termux, proot-distro, or remote
# Native Termux:   binaries in PATH (termux-battery-status etc.)
# proot-distro:    binaries at /data/data/com.termux/files/usr/bin/
# Remote:          must use SSH

if command -v termux-battery-status &>/dev/null && [ -n "${TERMUX_VERSION-}" ]; then
  echo "Native Termux — binaries directly available in PATH"
  TERMUX_PREFIX=""
elif [ -d /data/data/com.termux/files/usr/bin ] && ls /data/data/com.termux/files/usr/bin/termux-* &>/dev/null 2>&1; then
  echo "proot-distro — binaries at /data/data/com.termux/files/usr/bin"
  TERMUX_PREFIX="/data/data/com.termux/files/usr/bin"
else
  echo "No direct access — use SSH"
  TERMUX_PREFIX="ssh"
fi
```

```bash
# Helper function: auto-detect and run any termux command
termux-exec() {
  local cmd=$1; shift
  if command -v "$cmd" &>/dev/null 2>&1; then
    "$cmd" "$@"
  elif [ -x /data/data/com.termux/files/usr/bin/"$cmd" ]; then
    /data/data/com.termux/files/usr/bin/"$cmd" "$@"
  else
    ssh -p 8022 <device-ip> "$cmd $*"
  fi
}
```

## Direct Access (native Termux or proot-distro)

### Native Termux (opencode compilado corriendo directamente en Termux)
Los binarios termux-* ya están en el PATH automáticamente:

```bash
# Funciona directo — $PREFIX/bin ya está en PATH
termux-battery-status
termux-notification -t "Hola" -c "Mundo"
termux-location
```

### proot-distro (openCode desde una distro como Ubuntu)
Los binarios están en el directorio de Termux, no en PATH:

```bash
# Usar ruta completa
/data/data/com.termux/files/usr/bin/termux-battery-status

# O agregar al PATH
export PATH=$PATH:/data/data/com.termux/files/usr/bin
termux-battery-status
```

### Start API service
```bash
# Termux nativo
termux-api-start

# proot-distro
/data/data/com.termux/files/usr/bin/termux-api-start
```

## Remote Access via SSH

If on a different machine:

```bash
ssh -p 8022 <device-ip> '<termux-api-command>'
```

## Important Notes

1. **Termux must be in foreground** for camera/microphone commands
2. **Permissions must be granted** in Android Settings → Termux:API → Permissions
3. **Start API service** if commands timeout: `termux-api-start`

## API Commands Reference

### Device Info
| Command | Description |
|---------|-------------|
| `termux-battery-status` | Battery level, charging status, temperature |
| `termux-audio-info` | Audio device info |
| `termux-wifi-connectioninfo` | Current WiFi connection details |
| `termux-wifi-scaninfo` | Scan nearby WiFi networks |
| `termux-telephony-deviceinfo` | Phone/SIM info |
| `termux-telephony-cellinfo` | Cell tower info |
| `termux-sensor -l` | List available sensors |
| `termux-sensor -s <sensor> -n 1` | Read sensor once |

### Camera & Media
| Command | Description |
|---------|-------------|
| `termux-camera-info` | List cameras (id 0=back, 1=front) |
| `termux-camera-photo -c <id> <file.jpg>` | Take photo (needs foreground) |
| `termux-microphone-record -f <file>` | Record audio |
| `termux-media-player play <file>` | Play audio file |
| `termux-tts-speak "text"` | Text to speech |
| `termux-tts-engines` | List TTS engines |

### Notifications & Feedback
| Command | Description |
|---------|-------------|
| `termux-notification -t "title" -c "content"` | Show notification |
| `termux-notification-remove --id <id>` | Remove notification |
| `termux-toast "message"` | Show toast popup |
| `termux-vibrate -d <ms>` | Vibrate for duration |
| `termux-torch on/off` | Toggle flashlight |
| `termux-dialog` | Show dialog (various types) |

### Communication (needs permissions)
| Command | Description |
|---------|-------------|
| `termux-sms-list -l 10` | List recent SMS |
| `termux-sms-send -n <number> "message"` | Send SMS |
| `termux-contact-list` | List contacts |
| `termux-call-log -l 10` | Recent call history |
| `termux-telephony-call <number>` | Make phone call |

### Clipboard
| Command | Description |
|---------|-------------|
| `termux-clipboard-get` | Get clipboard content |
| `termux-clipboard-set "text"` | Set clipboard |

### Location
| Command | Description |
|---------|-------------|
| `termux-location` | Get GPS location (needs permission) |
| `termux-location -p gps` | Use GPS provider |
| `termux-location -p network` | Use network provider |

### System Control
| Command | Description |
|---------|-------------|
| `termux-volume` | Get/set volume levels |
| `termux-brightness <0-255>` | Set screen brightness |
| `termux-wallpaper -f <file>` | Set wallpaper |
| `termux-wake-lock` | Prevent sleep |
| `termux-wake-unlock` | Allow sleep |

### Storage & Sharing
| Command | Description |
|---------|-------------|
| `termux-share -a send <file>` | Share file via Android intent |
| `termux-open <file>` | Open file with default app |
| `termux-open-url <url>` | Open URL in browser |
| `termux-download <url>` | Download file |
| `termux-storage-get <dest>` | Pick file from storage |

### Biometrics & Security
| Command | Description |
|---------|-------------|
| `termux-fingerprint` | Authenticate with biometric sensor |
| `termux-keystore` | Access Android Keystore |
| `termux-usb` | List and interact with USB devices |

### NFC & Infrared
| Command | Description |
|---------|-------------|
| `termux-nfc` | Read NFC tags |
| `termux-infrared-frequencies` | List IR carrier frequencies |
| `termux-infrared-transmit` | Transmit IR signal |

### Audio & Speech
| Command | Description |
|---------|-------------|
| `termux-speech-to-text` | Voice recognition (requires permission) |
| `termux-media-scan` | Scan media files (e.g. after download) |

### Notifications (advanced)
| Command | Description |
|---------|-------------|
| `termux-notification-channel` | Create/manage notification channels (Android 8+) |
| `termux-notification-list` | List active notifications |

### SMS
| Command | Description |
|---------|-------------|
| `termux-sms-inbox` | Read SMS inbox messages |

### WiFi
| Command | Description |
|---------|-------------|
| `termux-wifi-enable <true/false>` | Enable or disable WiFi |

### Storage Access Framework (SAF)
| Command | Description |
|---------|-------------|
| `termux-saf-create <uri>` | Create a document in SAF tree |
| `termux-saf-dirs <uri>` | List directories in SAF tree |
| `termux-saf-ls <uri>` | List files in SAF tree |
| `termux-saf-managedir <uri>` | Create directory in SAF tree |
| `termux-saf-mkdir <uri>` | Create directory in SAF tree |
| `termux-saf-read <uri>` | Read file content from SAF tree |
| `termux-saf-rm <uri>` | Delete file from SAF tree |
| `termux-saf-stat <uri>` | Get file info from SAF tree |
| `termux-saf-write <uri>` | Write to file in SAF tree |

### Scheduling & Device Control
| Command | Description |
|---------|-------------|
| `termux-job-scheduler` | Schedule background tasks |
| `termux-api-start` | Start Termux:API service |
| `termux-api-stop` | Stop Termux:API service |
| `termux-reload-settings` | Reload Termux settings |

## Common Patterns

### Using the helper (auto-detect)
```bash
# Reusable helper — works in Termux nativo, proot-distro, y remoto
termux-exec() {
  local cmd=$1; shift
  if command -v "$cmd" &>/dev/null 2>&1; then
    "$cmd" "$@"                       # nativo
  elif [ -x /data/data/com.termux/files/usr/bin/"$cmd" ]; then
    /data/data/com.termux/files/usr/bin/"$cmd" "$@"  # proot
  else
    ssh -p 8022 <ip> "$cmd $*"        # remoto
  fi
}

# Usage examples
termux-exec termux-battery-status
termux-exec termux-notification -t "Alert" -c "Task complete"
termux-exec termux-camera-photo -c 1 ~/selfie.jpg
termux-exec termux-location -p network
termux-exec termux-battery-status | jq '.percentage, .status'
```

### Take a selfie
```bash
# Native Termux
termux-camera-photo -c 1 ~/selfie.jpg

# proot-distro
/data/data/com.termux/files/usr/bin/termux-camera-photo -c 1 ~/selfie.jpg

# Remote via SSH
ssh -p 8022 <ip> 'termux-camera-photo -c 1 ~/selfie.jpg'
scp -P 8022 <ip>:~/selfie.jpg /local/path/
```

### Send notification with action
```bash
termux-exec termux-notification -t "Alert" -c "Task complete" --id myalert --vibrate 200,100,200
```

### Get device location
```bash
termux-exec termux-location -p network
```

### Monitor battery
```bash
termux-exec termux-battery-status | jq '.percentage, .status'
```

## Troubleshooting

### Command times out
1. Start API service: `termux-api-start`
2. Check if Termux:API app is installed
3. For camera/mic: ensure Termux app is in foreground

### Permission denied
1. Open Android Settings → Apps → Termux:API → Permissions
2. Grant required permissions (camera, location, SMS, etc.)

### Direct path not found
```bash
# Verify binaries exist
ls -la /data/data/com.termux/files/usr/bin/termux-* 2>/dev/null || echo "Not in Termux environment"
```

### SSH connection refused
1. In Termux: `pkg install openssh && sshd`
2. SSH runs on port 8022 by default
3. Set password with `passwd` or add SSH key to `~/.ssh/authorized_keys`

## Command Combinations

### Interactive notification with buttons

```bash
NOTIF_ID=12345
PREFIX=/data/data/com.termux/files/usr

$PREFIX/bin/termux-notification \
  --id $NOTIF_ID \
  --title "Download Music" \
  --content "Choose source:" \
  --button1 "YouTube" \
  --button1-action "sh -c 'am startservice --user 0 \
    -n com.termux/com.termux.app.RunCommandService \
    -a com.termux.RUN_COMMAND \
    --es com.termux.RUN_COMMAND_PATH $HOME/scripts/yt-dlp.sh \
    --es com.termux.RUN_COMMAND_WORKDIR $HOME \
    --ez com.termux.RUN_COMMAND_BACKGROUND false \
    --es com.termux.RUN_COMMAND_SESSION_ACTION 0'" \
  --button2 "Spotify" \
  --button2-action "sh -c 'am startservice --user 0 \
    -n com.termux/com.termux.app.RunCommandService \
    -a com.termux.RUN_COMMAND \
    --es com.termux.RUN_COMMAND_PATH $HOME/scripts/zotify-download.sh \
    --es com.termux.RUN_COMMAND_WORKDIR $HOME \
    --ez com.termux.RUN_COMMAND_BACKGROUND false \
    --es com.termux.RUN_COMMAND_SESSION_ACTION 0'" \
  --button3 "Close" \
  --button3-action "$PREFIX/bin/termux-notification-remove $NOTIF_ID" \
  --ongoing
```

### Battery monitor

```bash
while true; do
  clear
  termux-exec termux-battery-status | jq '.percentage, .status, .temperature'
  sleep 60
done
```

### Take photo and share

```bash
PHOTO=~/photo_$(date +%s).jpg
termux-exec termux-camera-photo -c 0 "$PHOTO"
termux-exec termux-share -a send "$PHOTO"
```

### Get location and notify

```bash
LOC=$(termux-exec termux-location -p network | jq -r '.latitude, .longitude')
LAT=$(echo "$LOC" | head -1)
LON=$(echo "$LOC" | tail -1)
termux-exec termux-notification -t "Location" -c "$LAT, $LON" --id loc
```

### Flash alert on low battery

```bash
LEVEL=$(termux-exec termux-battery-status | jq '.percentage')
if [ "$LEVEL" -lt 20 ]; then
  for i in 1 2 3; do
    termux-exec termux-torch on
    sleep 0.5
    termux-exec termux-torch off
    sleep 0.5
  done
  termux-exec termux-notification -t "Battery Low" -c "$LEVEL% remaining"
fi
```

### Interactive URL/file opener (termux-url-opener / termux-file-editor)

Save as `~/bin/termux-url-opener` and symlink both:

```bash
ln -s ~/bin/termux-url-opener ~/bin/termux-file-editor
```

```bash
#!/data/data/com.termux/files/usr/bin/bash
cd ~/downloads
TMPFILE=dialog.tmp

function a {
  if which "$1" >/dev/null 2>&1; then
    M[${#M[*]}]="${3:-$1}"
    M[${#M[*]}]="${2:-$1}"
  fi
}

a w3m 'browser/viewer w3m'

if [[ $0 =~ -file-editor$ ]]; then
  T=file
  TN=name
  a sensible-editor
  a nano 'Nano Editor'
  a micro 'Micro Editor'
  a mcedit 'Midnight Commander Editor'
  a vim 'Vim'
  a vi 'Vi viewer/editor'
  a sensible-pager
  a more 'viewer more'
  a less 'viewer less'
  a proj 'add to project' 'proj file ""'
  a termux-share 'edit in app' 'termux-share -a edit'
  a termux-share 'send to app' 'termux-share -a send'
  a termux-share 'view in app'
elif [[ $0 =~ -url-opener$ ]]; then
  T=url
  a sensible-browser
  a elinks
  a lynx
  a proj bookmark 'project bookmark' 'proj url ""'

  if [[ "$1" == *"open.spotify.com"* ]]; then
    a spotdl "Download with spotdl" 'spotdl --bitrate "320k" --output "$HOME/storage/shared/spotdl/"'
  fi

  if [[ $1 =~ ^(ht|f)tp: ]] || ! [[ "$(wget 2>&1)" =~ ^BusyBox ]]; then
    a wget 'download with wget'
  fi
  a termux-open-url 'view in app'
else
  T=unknown
fi

a termux-clipboard-set "copy $T$TN to clipboard"
a termux-share "share $T$TN as text" 'termux-share -a send <<<'
a termux-open 'view in app' 'termux-open --chooser'
a termux-open 'send to app' 'termux-open --send --chooser'

dialog --title "Select Action" --menu "What to do with $T $1?" 0 0 0 "${M[@]}" 2>$TMPFILE

if [ $? = 0 ]; then
  eval "$(< $TMPFILE) '$1'"
elif [ -s $TMPFILE ]; then
  echo "Dialog error:"
  cat $TMPFILE
fi

rm -f $TMPFILE
```
