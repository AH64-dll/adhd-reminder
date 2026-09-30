import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

export default class ADHDHelper extends Extension {
    enable() {
        this._signals = [];
        const connect = (object, signal, callback) => {
            this._signals.push([object, object.connect(signal, callback)]);
        };
        connect(global.display, 'in-fullscreen-changed', () => this._report());
        connect(Main.layoutManager, 'monitors-changed', () => this._report());
        connect(Main.sessionMode, 'updated', () => this._report());
        connect(global.display, 'window-created', (_display, window) => {
            this._decorate(window);
        });
        for (const actor of global.get_window_actors()) this._decorate(actor.meta_window);
        this._timer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 15, () => {
            this._report();
            return GLib.SOURCE_CONTINUE;
        });
        this._report();
    }

    _decorate(window) {
        if (window?.get_wm_class() !== 'ADHD') return;
        window.make_above();
        window.stick();
        // Never activate the window: reminders must not take keyboard focus.
    }

    _report() {
        const fullscreen = Main.layoutManager.monitors.some(m => global.display.get_monitor_in_fullscreen(m.index));
        const locked = Main.sessionMode.isLocked;
        Gio.DBus.session.call('io.github.adhd.Desktop', '/io/github/adhd/Desktop',
            'io.github.adhd.Desktop', 'Report', new GLib.Variant('(bbs)', [fullscreen, locked, 'GNOME']),
            null, Gio.DBusCallFlags.NO_AUTO_START, 1500, null, (connection, result) => {
                try { connection.call_finish(result); } catch (_) { /* The app may be intentionally closed. */ }
            });
    }

    disable() {
        if (this._timer) GLib.source_remove(this._timer);
        this._timer = null;
        for (const [object, id] of this._signals ?? []) object.disconnect(id);
        this._signals = [];
    }
}
