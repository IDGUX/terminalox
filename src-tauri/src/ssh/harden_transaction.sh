set -euo pipefail
umask 077

action=$1
token=$2
connection=$3
login=$4
delay=$5
config=/etc/ssh/sshd_config
dropin=/etc/ssh/sshd_config.d/00-terminalox.conf
state=/etc/ssh/.terminalox-pending
lock=/etc/ssh/.terminalox.lock
timer=terminalox-rollback-$token

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

replace_file() {
    local source=$1 target=$2 temporary
    temporary=$(mktemp -d "${target%/*}/.terminalox.XXXXXXXX") || return 1
    if cp -a -- "$source" "$temporary/file" && mv -fT -- "$temporary/file" "$target"; then
        rmdir -- "$temporary"
    else
        rm -rf -- "$temporary"
        return 1
    fi
}

rollback() {
    local expected=$1 service
    exec 9>"$lock"
    flock 9
    if [[ ! -f "$state/token" || $(cat "$state/token") != "$expected" ]]; then
        return 0
    fi
    service=$(cat "$state/service")
    if [[ -f "$state/config" ]]; then
        replace_file "$state/config" "$config"
    fi
    if [[ -f "$state/dropin" ]]; then
        replace_file "$state/dropin" "$dropin"
    else
        rm -f -- "$dropin"
    fi
    if [[ -f "$state/directory-created" && -d ${dropin%/*} ]]; then
        rmdir --ignore-fail-on-non-empty -- "${dropin%/*}"
    fi
    sshd -t -f "$config"
    systemctl reload "$service"
    retire_state
}

retire_state() {
    local retired
    retired=$(mktemp -d /etc/ssh/.terminalox-finished.XXXXXXXX)
    mv -T -- "$state" "$retired/state"
    rm -rf -- "$retired" || true
}

check_auth() {
    local addr peer_port local_addr local_port extra effective hostname
    read -r addr peer_port local_addr local_port extra <<< "$connection"
    [[ -n "$addr" && "$peer_port" =~ ^[0-9]+$ && -n "$local_addr" &&
        "$local_port" =~ ^[0-9]+$ && -z "$extra" ]] || return 2

    effective=$(sshd -T -f "$config") || return 2
    hostname=$addr
    if grep -qx 'usedns yes' <<< "$effective"; then
        # Resolve the peer hostname as sshd does when UseDNS is enabled.
        hostname=$(getent hosts "$addr" | awk 'NR == 1 { print $2 }') || hostname=$addr
        if [[ -z "$hostname" ]] || ! getent ahosts "$hostname" |
            awk -v addr="$addr" '$1 == addr { found=1 } END { exit !found }'; then
            hostname=$addr
        fi
    fi
    effective=$(sshd -T -f "$config" -C "user=$login,host=$hostname,addr=$addr,laddr=$local_addr,lport=$local_port") ||
        return 2
    grep -qx 'passwordauthentication no' <<< "$effective" &&
        grep -qx 'kbdinteractiveauthentication no' <<< "$effective"
}

exec 9>"$lock"
flock -n 9 || fail 'Eine andere SSH-Konfigurationsänderung läuft bereits.'

case "$action" in
    apply)
        [[ ! -e "$state" ]] || fail 'Eine SSH-Härtung oder Wiederherstellung läuft bereits.'
        service=
        for candidate in ssh.service sshd.service; do
            if systemctl is-active --quiet "$candidate" &&
                [[ $(systemctl show -p CanReload --value "$candidate") == yes ]]; then
                service=$candidate
                break
            fi
        done
        [[ -n "$service" ]] || fail 'Für den Rollback ist ein aktiver, neu ladbarer systemd-SSH-Dienst erforderlich.'
        [[ -f "$config" && ! -L "$config" && ! -L "$dropin" ]] ||
            fail 'Symbolische Links in der SSH-Konfiguration werden für die Härtung nicht unterstützt.'
        [[ ! -e "$dropin" || -f "$dropin" ]] || fail 'Der SSH-Drop-in-Pfad ist keine normale Datei.'
        sshd -t -f "$config"

        staged=$(mktemp -d /etc/ssh/.terminalox-stage.XXXXXXXX)
        trap 'rm -rf -- "$staged"' EXIT
        printf '%s\n' "$token" > "$staged/token"
        printf '%s\n' "$service" > "$staged/service"
        if [[ -f "$dropin" ]]; then cp -a -- "$dropin" "$staged/dropin"; fi
        if [[ ! -d ${dropin%/*} ]]; then touch "$staged/directory-created"; fi
        if ! grep -qiE '^[[:space:]]*Include[[:space:]]+.*sshd_config\.d/\*\.conf' "$config"; then
            cp -a -- "$config" "$staged/config"
            cp -a -- "$config" "$staged/config.new"
            {
                printf '%s\n' '# BEGIN_TERMINALOX_INCLUDE' 'Include /etc/ssh/sshd_config.d/*.conf' '# END_TERMINALOX_INCLUDE'
                cat "$staged/config"
            } > "$staged/config.new"
        fi
        cat > "$staged/dropin.new" <<'DROPIN'
# Managed by terminalox — do not edit.
PasswordAuthentication no
KbdInteractiveAuthentication no
DROPIN
        chmod 0644 "$staged/dropin.new"
        {
            printf '%s\n' 'set -euo pipefail'
            declare -p config dropin state lock
            declare -f replace_file retire_state rollback
            printf '%s\n' 'rollback "$1"'
        } > "$staged/rollback.sh"

        command -v systemd-run >/dev/null || fail 'systemd-run ist für den SSH-Rollback erforderlich.'
        systemd-run --quiet --collect --unit="$timer" --on-active="${delay}s" \
            --timer-property=AccuracySec=1s --property=Restart=on-failure --property=RestartSec=10s \
            /bin/bash -c 'if [[ -f "$1/rollback.sh" ]]; then exec /bin/bash "$1/rollback.sh" "$2"; fi' \
            terminalox "$state" "$token" 9>&-
        mv -T -- "$staged" "$state"
        printf '%s\n' 'STEP:rollback_armed'
        if [[ -f "$state/config.new" ]]; then replace_file "$state/config.new" "$config"; fi
        if [[ ! -d ${dropin%/*} ]]; then install -d -m 0755 "${dropin%/*}"; fi
        replace_file "$state/dropin.new" "$dropin"
        printf '%s\n' 'STEP:dropin_installed'
        sshd -t -f "$config"
        check_auth || fail 'Die beiden Passwortverfahren sind für diese Verbindung nicht nachweislich deaktiviert.'
        printf '%s\n' 'STEP:sshd_validated'
        systemctl reload "$service"
        printf '%s\n' 'STEP:sshd_reloaded'
        ;;
    confirm)
        [[ -f "$state/token" && $(cat "$state/token") == "$token" ]] ||
            fail 'Die SSH-Härtung ist nicht mehr offen. Der Rollback wurde möglicherweise bereits ausgeführt.'
        cmp -s "$state/dropin.new" "$dropin" || fail 'SSH-Drop-in wurde während der Prüfung geändert.'
        check_auth || fail 'Die beiden Passwortverfahren sind für die neue Verbindung nicht nachweislich deaktiviert.'
        retire_state
        # If the timer already fired, its service will find no matching state.
        systemctl stop "$timer.timer" >/dev/null 2>&1 || true
        printf '%s\n' 'STEP:password_auth_off'
        ;;
    verify)
        [[ ! -e "$state" ]] || fail 'Eine SSH-Härtung oder Wiederherstellung läuft bereits.'
        if check_auth; then
            printf '%s\n' 'EFFECTIVE:off'
        elif [[ $? == 1 ]]; then
            printf '%s\n' 'EFFECTIVE:on'
        else
            printf '%s\n' 'EFFECTIVE:unknown'
        fi
        ;;
    unharden)
        [[ ! -e "$state" ]] || fail 'Eine SSH-Härtung oder Wiederherstellung läuft bereits.'
        rm -f -- "$dropin" /etc/ssh/sshd_config.d/99-terminalox.conf
        if grep -qx '# BEGIN_TERMINALOX_INCLUDE' "$config"; then
            staged=$(mktemp -d /etc/ssh/.terminalox-stage.XXXXXXXX)
            trap 'rm -rf -- "$staged"' EXIT
            cp -a -- "$config" "$staged/config"
            awk '/^# BEGIN_TERMINALOX_INCLUDE$/{skip=1; next} /^# END_TERMINALOX_INCLUDE$/{skip=0; next} !skip' \
                "$config" > "$staged/config"
            replace_file "$staged/config" "$config"
        fi
        sshd -t -f "$config"
        for service in ssh sshd; do systemctl reload "$service" 2>/dev/null && exit 0; done
        for service in ssh sshd; do service "$service" reload 2>/dev/null && exit 0; done
        for service in sshd ssh; do
            rc-service "$service" reload 2>/dev/null && exit 0
            rc-service "$service" restart 2>/dev/null && exit 0
        done
        fail 'STEP:reload_failed'
        ;;
    *)
        fail 'Unbekannte SSH-Härtungsaktion.'
        ;;
esac
