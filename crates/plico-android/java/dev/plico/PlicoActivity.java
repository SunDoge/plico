package dev.plico;

import android.app.NativeActivity;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.text.Editable;
import android.text.Selection;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.*;
import android.widget.FrameLayout;

/** The surface keeps rendering Plico; this non-drawing view serves Android's IME. */
public class PlicoActivity extends NativeActivity {
    private Editor editor;
    private View surfaceFocus;
    private long session;
    private String initial;
    private int cursor, anchor, type;
    private static native void edit(long session, int kind, String text, int before, int after);

    @Override public void onCreate(Bundle state) {
        try {
            Bundle metadata = getPackageManager().getActivityInfo(getComponentName(),
                    PackageManager.GET_META_DATA).metaData;
            System.loadLibrary(metadata.getString("android.app.lib_name"));
        } catch (PackageManager.NameNotFoundException e) { throw new IllegalStateException(e); }
        super.onCreate(state);
        editor = new Editor(this);
        editor.setFocusableInTouchMode(true);
        editor.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        addContentView(editor, new FrameLayout.LayoutParams(1, 1));
    }

    public void plicoEditor(long id, String text, int caret, int mark, int inputType) {
        runOnUiThread(() -> {
            if (isFinishing() || editor == null) return;
            InputMethodManager im = getSystemService(InputMethodManager.class);
            if (session == 0 && id != 0) surfaceFocus = getCurrentFocus();
            session = id;
            initial = text;
            cursor = caret;
            anchor = mark;
            type = inputType;
            if (id == 0) {
                im.hideSoftInputFromWindow(editor.getWindowToken(), 0);
                editor.clearFocus();
                if (surfaceFocus != null) surfaceFocus.requestFocus();
                surfaceFocus = null;
            } else {
                editor.requestFocus();
                im.restartInput(editor);
                im.showSoftInput(editor, InputMethodManager.SHOW_IMPLICIT);
            }
        });
    }

    private final class Editor extends View {
        Editor(Context context) { super(context); }
        @Override public boolean onCheckIsTextEditor() { return session != 0; }
        @Override public InputConnection onCreateInputConnection(EditorInfo info) {
            if (session == 0) return null;
            final long id = session;
            info.inputType = type;
            info.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI;
            info.initialSelStart = anchor;
            info.initialSelEnd = cursor;
            BaseInputConnection connection = new BaseInputConnection(this, true) {
                private String composing = "";
                @Override public boolean setComposingText(CharSequence s, int pos) {
                    if (id != session) return false;
                    composing = s.toString();
                    edit(id, 0, composing, 0, 0);
                    return super.setComposingText(s, pos);
                }
                @Override public boolean commitText(CharSequence s, int pos) {
                    if (id != session) return false;
                    composing = "";
                    edit(id, 1, s.toString(), 0, 0);
                    return super.commitText(s, pos);
                }
                @Override public boolean finishComposingText() {
                    if (id == session && !composing.isEmpty()) edit(id, 1, composing, 0, 0);
                    composing = "";
                    return super.finishComposingText();
                }
                @Override public boolean deleteSurroundingText(int before, int after) {
                    if (id != session) return false;
                    int b = getTextBeforeCursor(before, 0).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8).length;
                    int a = getTextAfterCursor(after, 0).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8).length;
                    edit(id, 2, "", b, a);
                    return super.deleteSurroundingText(before, after);
                }
            };
            Editable buffer = connection.getEditable();
            buffer.replace(0, buffer.length(), initial);
            Selection.setSelection(buffer, anchor, cursor);
            return connection;
        }
    }
}
