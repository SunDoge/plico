using Plico;
using Plico.Interop;

internal static unsafe class BoundaryChecks
{
    internal static void Run()
    {
        var ui = new Ui();
        Frame frame = new() { version = 5, width = 100, height = 100 };
        Reject(ui, null);
        frame.utf8_len = 16 * 1024 * 1024 + 1; Reject(ui, &frame); frame.utf8_len = 0;
        frame.version = 2; Reject(ui, &frame); frame.version = 5;
        frame.event_count = 1; Reject(ui, &frame);
        Event* events = stackalloc Event[2];
        events[0] = new() { id = 1, kind = 4, text_len = 2 };
        frame.events = events;
        byte* bytes = stackalloc byte[1]; bytes[0] = 0xff;
        frame.utf8 = bytes; frame.utf8_len = 1;
        Reject(ui, &frame); // Span must be validated before decoding.
        events[0].text_len = 1; Reject(ui, &frame); // Strict UTF-8.
        events[0] = new() { id = 1, kind = 1 };
        events[1] = events[0]; frame.event_count = 2;
        Reject(ui, &frame);
        frame.event_count = 1; events[0].kind = 999; Reject(ui, &frame);
        events[0].kind = 1; frame.seconds = double.NaN; Reject(ui, &frame);
        frame.seconds = 0; frame.event_count = 0; frame.utf8_len = 0;
        ui.Begin(&frame, true); ulong button = ui.Button("Boundary", key: "boundary").Id; _ = ui.Finish(); ui.EndBuild();
        events[0] = new() { id = button, kind = 1 }; frame.event_count = 1;
        ui.Begin(&frame, true);
        if (!ui.Button("Boundary", key: "boundary").Clicked()) throw new Exception("Valid activation did not cross the frame boundary");
        _ = ui.Finish(); ui.EndBuild();
        frame.event_count = 0;
        ui.Begin(&frame, true);
        ui.Button("Primary").Primary().FontSize(20);
        ui.Svg("<svg xmlns='http://www.w3.org/2000/svg'/>");
        uint onAccent = ui.Theme.OnAccent;
        var composed = ui.Finish();
        bool labelFound = false;
        for (int i = 0; i < composed.command_count; i++)
        {
            var command = composed.commands[i];
            if (command.kind == 3) {
                labelFound = true;
                if (command.foreground != onAccent || command.font_size != 20)
                    throw new Exception("Button label lost inherited foreground or font size");
            }
            if (command.kind == 11 && (command.flags & 8) != 0) throw new Exception("Untinted SVG received an implicit tint");
        }
        ui.EndBuild();
        if (!labelFound) throw new Exception("Missing button label");
        Console.Error.WriteLine("Boundary checks passed: frame validation, activation, font/color inheritance and SVG palette");
    }
    private static void Reject(Ui ui, Frame* frame)
    {
        bool rejected = false;
        try { ui.Begin(frame, true); }
        catch (InvalidOperationException) { rejected = true; }
        finally { ui.EndBuild(); }
        if (!rejected) throw new Exception("Invalid native frame was accepted");
    }
}
