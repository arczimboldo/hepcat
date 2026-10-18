// Global shortcuts: what they can do, registering their keys with the
// desktop, and running them. Each shortcut is
// { id, keys, target, targetLabel, action, param }, where `target` is a strip
// uid (`mic:…`, `app:…`, `bus:<name>`, `out:<node.name>`) or "hepcat".

import QtQuick

QtObject {
    id: shortcuts

    required property var mixer
    // Latest state snapshot from the backend.
    property var state: null
    // JSON array of shortcuts, stored in the layout settings.
    property string stored: "[]"
    signal storedEdited(string json)
    signal toggleWindowRequested()

    readonly property bool available: mixer.shortcutsAvailable()
    readonly property var list: {
        try {
            return JSON.parse(stored) || []
        } catch (e) {
            return []
        }
    }

    readonly property var kindNames: ({ mic: "Input", app: "App", bus: "Bus", output: "Output", hepcat: "Hepcat" })

    // What a shortcut can do, and to which kinds of strip. `param` asks for
    // one more choice: a step or level in percent, an effect, a bus or a device.
    readonly property var actions: [
        { id: "mute_toggle", label: "Mute or unmute", kinds: ["mic", "app", "bus", "output"] },
        { id: "mute", label: "Mute", kinds: ["mic", "app", "bus", "output"] },
        { id: "unmute", label: "Unmute", kinds: ["mic", "app", "bus", "output"] },
        { id: "hold_unmute", label: "Unmute while held (push to talk)", kinds: ["mic", "app", "bus", "output"] },
        { id: "hold_mute", label: "Mute while held", kinds: ["mic", "app", "bus", "output"] },
        { id: "volume_up", label: "Volume up", param: "step", kinds: ["mic", "app", "bus", "output"] },
        { id: "volume_down", label: "Volume down", param: "step", kinds: ["mic", "app", "bus", "output"] },
        { id: "volume_set", label: "Set volume", param: "level", kinds: ["mic", "app", "bus", "output"] },
        { id: "default", label: "Make default", kinds: ["mic", "bus", "output"] },
        { id: "fx_toggle", label: "Effects on or off", kinds: ["mic", "app", "bus"] },
        { id: "fx_option", label: "One effect on or off", param: "effect", kinds: ["mic", "app", "bus"] },
        { id: "route_toggle", label: "Send to a bus or stop", param: "bus", kinds: ["mic", "app"] },
        { id: "bus_output", label: "Play to a device", param: "output", kinds: ["bus"] },
        { id: "bus_mic", label: "Offer as microphone or stop", kinds: ["bus"] },
        { id: "window", label: "Show or hide the window", kinds: ["hepcat"] },
    ]

    readonly property var effects: [
        { id: "noise:standard", label: "Noise suppression" },
        { id: "noise:voice", label: "Voice isolation" },
        { id: "echo", label: "Echo cancellation" },
        { id: "gate", label: "Noise gate" },
        { id: "auto_gain", label: "Automatic gain control" },
        { id: "voice:robot", label: "Voice effect: Robot" },
        { id: "voice:alien", label: "Voice effect: Alien" },
        { id: "voice:radio", label: "Voice effect: Radio" },
        { id: "voice:telephone", label: "Voice effect: Telephone" },
        { id: "voice:megaphone", label: "Voice effect: Megaphone" },
        { id: "voice:underwater", label: "Voice effect: Underwater" },
        { id: "voice:echo", label: "Voice effect: Echo" },
        { id: "voice:hall", label: "Voice effect: Hall" },
    ]

    function action(id) {
        return actions.find(a => a.id === id)
    }

    // Every strip a shortcut can point at, hidden ones included.
    function targets() {
        const s = state
        const list = [{ uid: "hepcat", kind: "hepcat", label: "Hepcat window" }]
        if (!s)
            return list
        s.mics.forEach(m => list.push({ uid: m.key, kind: "mic", label: m.label }))
        s.apps.forEach(a => list.push({ uid: a.key, kind: "app", label: a.label }))
        s.buses.forEach(b => list.push({ uid: "bus:" + b.name, kind: "bus", label: b.label }))
        s.outputs.forEach(o => list.push({ uid: "out:" + o.name, kind: "output", label: o.label }))
        return list
    }

    function find(uid) {
        const s = state
        if (!s)
            return null
        const m = s.mics.find(i => i.key === uid)
        if (m) return { kind: "mic", item: m, fxKey: m.key }
        const a = s.apps.find(i => i.key === uid)
        if (a) return { kind: "app", item: a, fxKey: a.key }
        const b = s.buses.find(i => "bus:" + i.name === uid)
        if (b) return { kind: "bus", item: b, fxKey: uid }
        const o = s.outputs.find(i => "out:" + i.name === uid)
        if (o) return { kind: "output", item: o }
        return null
    }

    function paramLabel(sc) {
        const a = action(sc.action)
        if (!a || !a.param)
            return ""
        switch (a.param) {
        case "step":
        case "level":
            return (sc.param ?? (a.param === "step" ? 5 : 100)) + "%"
        case "effect":
            return (effects.find(e => e.id === sc.param) || { label: sc.param }).label
        case "bus": {
            const b = state && state.buses.find(i => i.name === sc.param)
            return b ? b.label : sc.param
        }
        case "output": {
            const o = state && state.outputs.find(i => i.name === sc.param)
            return o ? o.label : (sc.paramLabel || sc.param)
        }
        }
        return ""
    }

    // "Volume up 5%: Discord"
    function describe(sc) {
        const a = action(sc.action)
        const p = paramLabel(sc)
        const what = (a ? a.label : sc.action) + (p ? " (" + p + ")" : "")
        return sc.target === "hepcat" ? what : what + ": " + sc.targetLabel
    }

    function save(list) {
        storedEdited(JSON.stringify(list))
    }

    function register(sc) {
        return available && mixer.setShortcut(sc.id, describe(sc), sc.keys)
    }

    function registerAll() {
        list.forEach(register)
    }

    function put(sc) {
        const l = list.filter(x => x.id !== sc.id)
        const at = list.findIndex(x => x.id === sc.id)
        l.splice(at < 0 ? l.length : at, 0, sc)
        save(l)
        register(sc)
    }

    function remove(id) {
        mixer.removeShortcut(id)
        save(list.filter(x => x.id !== id))
    }

    // Changed in System Settings → Shortcuts.
    function keysChanged(id, keys) {
        const l = list.map(x => x.id === id ? Object.assign({}, x, { keys: keys }) : x)
        save(l)
    }

    function run(id, pressed) {
        const sc = list.find(x => x.id === id)
        if (!sc)
            return
        if (sc.action === "window") {
            if (pressed)
                toggleWindowRequested()
            return
        }
        const t = find(sc.target)
        if (!t)
            return
        const item = t.item
        const ids = (item.ids || []).join(",")
        const holds = sc.action === "hold_unmute" || sc.action === "hold_mute"
        // Only push-to-talk style actions do something on release.
        if (!pressed && !holds)
            return

        switch (sc.action) {
        case "mute_toggle": return ids && mixer.setMute(ids, !item.mute)
        case "mute": return ids && mixer.setMute(ids, true)
        case "unmute": return ids && mixer.setMute(ids, false)
        case "hold_unmute": return ids && mixer.setMute(ids, !pressed)
        case "hold_mute": return ids && mixer.setMute(ids, pressed)
        case "volume_up":
        case "volume_down": {
            const step = (sc.param ?? 5) / 100 * (sc.action === "volume_up" ? 1 : -1)
            return ids && mixer.setVolume(ids, Math.max(0, Math.min(1, item.volume + step)))
        }
        case "volume_set": return ids && mixer.setVolume(ids, (sc.param ?? 100) / 100)
        case "default":
            if (t.kind === "mic")
                return mixer.setDefaultSource(item.fxActive && item.fxDevice ? item.fxDevice : item.name)
            return mixer.setDefaultSink(t.kind === "bus" ? item.nodeName : item.name)
        case "fx_toggle": {
            const f = JSON.parse(item.fx || "{}")
            f.enabled = !f.enabled
            return mixer.setFx(t.fxKey, JSON.stringify(f))
        }
        case "fx_option": {
            const f = JSON.parse(item.fx || "{}")
            const [name, value] = String(sc.param).split(":")
            const on = f.enabled === true && (value === undefined ? f[name] === true : f[name] === value)
            if (value === undefined)
                f[name] = !on
            else
                f[name] = on ? (name === "noise" ? "off" : "none") : value
            // Switching an effect on also switches the strip's effects on.
            if (!on)
                f.enabled = true
            return mixer.setFx(t.fxKey, JSON.stringify(f))
        }
        case "route_toggle": return mixer.toggleRoute(item.key, sc.param)
        case "bus_output": return mixer.setBusOutput(item.name, sc.param)
        case "bus_mic": return mixer.setBusMic(item.name, !item.mic)
        }
    }
}
