import Gio from 'gi://Gio';

const BUS_NAME = 'dev.samuelba.Dropshade.Shell';
const OBJECT_PATH = '/dev/samuelba/Dropshade/Shell';
const INTERFACE = `
<node>
  <interface name="${BUS_NAME}">
    <method name="Toggle"/>
    <method name="Show"/>
    <method name="Hide"/>
    <method name="ToggleMaximize"/>
    <property name="IsManaged" type="b" access="read"/>
  </interface>
</node>`;

/** Lets the app and the command line drive the drop-down through D-Bus. */
export class ShellService {
    constructor({toggle, show, hide, toggleMaximize, isManaged}) {
        this._exported = Gio.DBusExportedObject.wrapJSObject(INTERFACE, {
            Toggle: toggle,
            Show: show,
            Hide: hide,
            ToggleMaximize: toggleMaximize,
            get IsManaged() {
                return isManaged();
            },
        });
        this._exported.export(Gio.DBus.session, OBJECT_PATH);
        this._nameId = Gio.bus_own_name_on_connection(
            Gio.DBus.session, BUS_NAME, Gio.BusNameOwnerFlags.NONE, null, null);
    }

    destroy() {
        Gio.bus_unown_name(this._nameId);
        this._exported.unexport();
    }
}
