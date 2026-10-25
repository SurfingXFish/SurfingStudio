// 所属模块：platforms/android / 活动入口
// 文件用途：
// - 继承 `NativeActivity`，只为 Storage Access Framework（SAF）补充两件事：
//   1. `onActivityResult` 转发；2. 查询所选文档的显示名（Rust 侧做 Cursor 查询过于繁琐）。
// - 其余全部逻辑仍在 Rust（`android_main` 与库中）。
// Module: platforms/android / activity entry
// Purpose:
// - Extend `NativeActivity` only for the Storage Access Framework (SAF):
//   1. forward `onActivityResult`; 2. resolve the picked document's display name (a Cursor query
//   would be far more verbose from Rust).
// - Everything else stays in Rust (`android_main` and the library).
package com.surfingstudio.app;

import android.app.NativeActivity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;

public class MainActivity extends NativeActivity {
    /** 请求码 1001：打开并导入一个文件（SAF ACTION_OPEN_DOCUMENT）。 */
    public static final int REQUEST_OPEN_DOCUMENT = 1001;
    /** 请求码 1002：在公共目录创建并写入一个文件（SAF ACTION_CREATE_DOCUMENT）。 */
    public static final int REQUEST_CREATE_DOCUMENT = 1002;

    static {
        // NativeActivity 只用 dlopen 加载 .so，JVM 并不知道这些原生方法在哪；显式 loadLibrary 后
        // JNI 才能解析下面的 nativeOnActivityResult。库名必须与清单里的 android.app.lib_name 一致。
        // NativeActivity only dlopen()s the .so, so the JVM cannot resolve native methods; calling
        // loadLibrary explicitly registers the library with this class loader. The name must match
        // android.app.lib_name in the manifest.
        System.loadLibrary("surfing_studio_android");
    }

    /** Rust 侧实现：把结果放进进程内队列，由等待中的工作线程取走。 */
    private static native void nativeOnActivityResult(
            int requestCode, int resultCode, String uri, String displayName);

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        Uri uri = data != null ? data.getData() : null;
        String name = uri != null ? queryDisplayName(uri) : null;
        nativeOnActivityResult(
                requestCode, resultCode, uri != null ? uri.toString() : null, name);
    }

    /** 读取 SAF 文档的显示名；失败时返回 null，Rust 侧会回退到按时间戳命名。 */
    private String queryDisplayName(Uri uri) {
        try (Cursor cursor =
                getContentResolver()
                        .query(uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                int index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (index >= 0) {
                    return cursor.getString(index);
                }
            }
        } catch (Exception ignored) {
            // 某些提供方不支持查询显示名，交给 Rust 侧回退。
            // Some providers do not expose a display name; Rust falls back to a timestamp.
        }
        return null;
    }
}
