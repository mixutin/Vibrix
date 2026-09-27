# Shared, source-only QEMU/OVMF discovery. Keep the interactive runner and
# headless smoke test on the same firmware-selection policy.
#
# Optional overrides:
#   OVMF_CODE=/path/to/OVMF_CODE.fd OVMF_VARS=/path/to/OVMF_VARS.fd
#   OVMF_SEARCH_DIRS=/custom/dir:/another/dir (also used by regression tests)
vibrix_find_ovmf() {
  local code_override="${OVMF_CODE:-}"
  local vars_override="${OVMF_VARS:-}"
  local candidate directory name
  local -a directories=()

  if [[ -n "${OVMF_SEARCH_DIRS:-}" ]]; then
    IFS=':' read -r -a directories <<< "$OVMF_SEARCH_DIRS"
  else
    directories=(
      /usr/share/OVMF
      /usr/share/edk2/x64
      /usr/share/edk2/ovmf
      /usr/share/edk2-ovmf/x64
      /usr/share/edk2-ovmf
      /usr/share/ovmf
      /usr/share/qemu
    )
  fi

  if [[ -n "$code_override" ]]; then
    if [[ ! -f "$code_override" || ! -r "$code_override" ]]; then
      printf '[vibrix] OVMF_CODE is not a readable file: %s\n' "$code_override" >&2
      return 1
    fi
    OVMF_CODE="$code_override"
  else
    OVMF_CODE=""
    # Only pair code/vars images from matching firmware-size families.
    for name in OVMF_CODE_4M.fd OVMF_CODE.4m.fd OVMF_CODE.fd OVMF_CODE_2M.fd OVMF_CODE.2m.fd; do
      for directory in "${directories[@]}"; do
        [[ -n "$directory" ]] || continue
        candidate="$directory/$name"
        if [[ -f "$candidate" && -r "$candidate" ]]; then
          OVMF_CODE="$candidate"
          break 2
        fi
      done
    done
  fi

  if [[ -z "$OVMF_CODE" ]]; then
    cat >&2 <<'EOF'
[vibrix] readable OVMF firmware not found.
[vibrix] Install QEMU firmware on the host:
[vibrix]   Ubuntu/Debian: sudo apt install qemu-system-x86 ovmf
[vibrix]   Fedora:        sudo dnf install qemu-system-x86 edk2-ovmf
[vibrix]   Arch:          sudo pacman -S qemu-system-x86 edk2-ovmf
[vibrix] Or pass OVMF_CODE=/path/to/OVMF_CODE.fd OVMF_VARS=/path/to/OVMF_VARS.fd.
EOF
    return 1
  fi

  if [[ -n "$vars_override" ]]; then
    if [[ ! -f "$vars_override" || ! -r "$vars_override" ]]; then
      printf '[vibrix] OVMF_VARS is not a readable file: %s\n' "$vars_override" >&2
      return 1
    fi
    OVMF_VARS="$vars_override"
    return 0
  fi

  case "${OVMF_CODE##*/}" in
    OVMF_CODE_4M.fd) name=OVMF_VARS_4M.fd ;;
    OVMF_CODE.4m.fd) name=OVMF_VARS.4m.fd ;;
    OVMF_CODE.fd) name=OVMF_VARS.fd ;;
    OVMF_CODE_2M.fd) name=OVMF_VARS_2M.fd ;;
    OVMF_CODE.2m.fd) name=OVMF_VARS.2m.fd ;;
    *)
      printf '[vibrix] Cannot infer matching OVMF_VARS for %s; set OVMF_VARS explicitly.\n' "$OVMF_CODE" >&2
      return 1
      ;;
  esac

  OVMF_VARS=""
  # First prefer the VARS image distributed beside the selected CODE image.
  candidate="${OVMF_CODE%/*}/$name"
  if [[ -f "$candidate" && -r "$candidate" ]]; then
    OVMF_VARS="$candidate"
  else
    for directory in "${directories[@]}"; do
      [[ -n "$directory" ]] || continue
      candidate="$directory/$name"
      if [[ -f "$candidate" && -r "$candidate" ]]; then
        OVMF_VARS="$candidate"
        break
      fi
    done
  fi

  if [[ -z "$OVMF_VARS" ]]; then
    printf '[vibrix] readable %s not found for %s.\n' "$name" "$OVMF_CODE" >&2
    printf '[vibrix] Install the matching OVMF VARS firmware or set OVMF_VARS explicitly.\n' >&2
    return 1
  fi
}
