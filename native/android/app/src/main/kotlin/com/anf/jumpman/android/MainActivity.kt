package com.anf.jumpman.android

import android.hardware.input.InputManager
import android.os.Bundle
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import com.google.androidgamesdk.GameActivity

/** JNI entry points implemented in `native/src/android.rs`. */
object Native {
    /** kind: 0 connect, 1 disconnect, 2 button, 3 axis (same layout as the iOS GameController shim; see controller/map.rs). */
    @JvmStatic
    external fun padEvent(kind: Int, uid: Int, id: Int, value: Float)
}

/**
 * Fullscreen landscape GameActivity. Gamepad / joystick events (the Retroid Pocket's built-in controls arrive as a
 * standard Android gamepad) are taken here and forwarded to Rust, because winit's Android backend does not deliver
 * joystick axes and gilrs has no Android backend. Touch input still goes through GameActivity to Bevy.
 */
class MainActivity : GameActivity() {
    private val known = HashSet<Int>()
    private val lastAxis = HashMap<Long, Float>()
    private val hatX = HashMap<Int, Float>()
    private val hatY = HashMap<Int, Float>()

    private val deviceListener = object : InputManager.InputDeviceListener {
        override fun onInputDeviceAdded(deviceId: Int) = connectIfPad(deviceId)
        override fun onInputDeviceChanged(deviceId: Int) = connectIfPad(deviceId)
        override fun onInputDeviceRemoved(deviceId: Int) {
            if (known.remove(deviceId)) Native.padEvent(1, deviceId, 0, 0f)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        WindowCompat.setDecorFitsSystemWindows(window, false)
        val im = getSystemService(INPUT_SERVICE) as InputManager
        im.registerInputDeviceListener(deviceListener, null)
        for (id in InputDevice.getDeviceIds()) connectIfPad(id)
    }

    override fun onDestroy() {
        (getSystemService(INPUT_SERVICE) as InputManager).unregisterInputDeviceListener(deviceListener)
        super.onDestroy()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) hideSystemUi()
    }

    private fun hideSystemUi() {
        window.decorView.post {
            val controller = WindowCompat.getInsetsController(window, window.decorView)
            controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            controller.hide(WindowInsetsCompat.Type.systemBars())
        }
    }

    private fun isPad(d: InputDevice?): Boolean {
        if (d == null || d.isVirtual) return false
        val s = d.sources
        return (s and InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD ||
            (s and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK
    }

    private fun adopt(deviceId: Int) {
        if (deviceId >= 0 && known.add(deviceId)) Native.padEvent(0, deviceId, 0, 0f)
    }

    private fun connectIfPad(deviceId: Int) {
        if (deviceId < 0 || known.contains(deviceId)) return
        if (isPad(InputDevice.getDevice(deviceId))) adopt(deviceId)
    }

    /** Element ids as in controller/map.rs: 0 A(south) 1 B(east) 2 X(west) 3 Y(north) 4 L1 5 R1 6 L2 7 R2 8-11 dpad U/D/L/R 12 Start 13 Select 14 Home 15 L3 16 R3. */
    private fun element(keyCode: Int): Int = when (keyCode) {
        KeyEvent.KEYCODE_BUTTON_A -> 0
        KeyEvent.KEYCODE_BUTTON_B -> 1
        KeyEvent.KEYCODE_BUTTON_X -> 2
        KeyEvent.KEYCODE_BUTTON_Y -> 3
        KeyEvent.KEYCODE_BUTTON_L1 -> 4
        KeyEvent.KEYCODE_BUTTON_R1 -> 5
        KeyEvent.KEYCODE_BUTTON_L2 -> 6
        KeyEvent.KEYCODE_BUTTON_R2 -> 7
        KeyEvent.KEYCODE_DPAD_UP -> 8
        KeyEvent.KEYCODE_DPAD_DOWN -> 9
        KeyEvent.KEYCODE_DPAD_LEFT -> 10
        KeyEvent.KEYCODE_DPAD_RIGHT -> 11
        KeyEvent.KEYCODE_BUTTON_START -> 12
        KeyEvent.KEYCODE_BUTTON_SELECT -> 13
        KeyEvent.KEYCODE_BUTTON_MODE -> 14
        KeyEvent.KEYCODE_BUTTON_THUMBL -> 15
        KeyEvent.KEYCODE_BUTTON_THUMBR -> 16
        else -> -1
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val src = event.source
        val fromPad = (src and InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD ||
            (src and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK ||
            ((src and InputDevice.SOURCE_DPAD) == InputDevice.SOURCE_DPAD && isPad(event.device))
        if (fromPad && event.keyCode != KeyEvent.KEYCODE_BACK) {
            val id = element(event.keyCode)
            if (id >= 0) {
                // Some handheld drivers report a pad that isPad() did not list: adopt it on first use.
                adopt(event.deviceId)
                if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount > 0) return true
                if (event.action == KeyEvent.ACTION_DOWN) Native.padEvent(2, event.deviceId, id, 1f)
                else if (event.action == KeyEvent.ACTION_UP) Native.padEvent(2, event.deviceId, id, 0f)
                return true
            }
        }
        return super.dispatchKeyEvent(event)
    }

    private fun axis(deviceId: Int, id: Int, androidAxis: Int, value: Float) {
        val key = (deviceId.toLong() shl 8) or androidAxis.toLong()
        if (lastAxis[key] == value) return
        lastAxis[key] = value
        Native.padEvent(3, deviceId, id, value)
    }

    private fun trigger(d: Int, id: Int, androidAxis: Int, v: Float) {
        val key = (d.toLong() shl 8) or (androidAxis.toLong() + 100)
        if (lastAxis[key] == v) return
        lastAxis[key] = v
        Native.padEvent(2, d, id, v)
    }

    /** `neg`/`pos` are element ids (left/right or up/down); `value` is -1, 0 or 1. */
    private fun hat(d: Int, neg: Int, pos: Int, state: HashMap<Int, Float>, value: Float) {
        val prev = state[d] ?: 0f
        if (prev == value) return
        state[d] = value
        if (prev < 0f && value >= 0f) Native.padEvent(2, d, neg, 0f)
        if (prev > 0f && value <= 0f) Native.padEvent(2, d, pos, 0f)
        if (value < 0f && prev >= 0f) Native.padEvent(2, d, neg, 1f)
        if (value > 0f && prev <= 0f) Native.padEvent(2, d, pos, 1f)
    }

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean {
        if ((event.source and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK && event.action == MotionEvent.ACTION_MOVE) {
            val d = event.deviceId
            adopt(d)
            // The game's axis ids have Y up, Android has Y down.
            axis(d, 0, MotionEvent.AXIS_X, event.getAxisValue(MotionEvent.AXIS_X))
            axis(d, 1, MotionEvent.AXIS_Y, -event.getAxisValue(MotionEvent.AXIS_Y))
            // Right stick is Z/RZ on most pads, RX/RY on some.
            val useZ = event.getAxisValue(MotionEvent.AXIS_Z) != 0f || event.getAxisValue(MotionEvent.AXIS_RZ) != 0f
            val rx = if (useZ) MotionEvent.AXIS_Z else MotionEvent.AXIS_RX
            val ry = if (useZ) MotionEvent.AXIS_RZ else MotionEvent.AXIS_RY
            axis(d, 2, rx, event.getAxisValue(rx))
            axis(d, 3, ry, -event.getAxisValue(ry))
            // Analog triggers (pads that report them as axes rather than L2/R2 keys).
            trigger(d, 6, MotionEvent.AXIS_LTRIGGER, maxOf(event.getAxisValue(MotionEvent.AXIS_LTRIGGER), event.getAxisValue(MotionEvent.AXIS_BRAKE)))
            trigger(d, 7, MotionEvent.AXIS_RTRIGGER, maxOf(event.getAxisValue(MotionEvent.AXIS_RTRIGGER), event.getAxisValue(MotionEvent.AXIS_GAS)))
            // D-pad reported as a hat (up is -1 on Y).
            hat(d, 10, 11, hatX, event.getAxisValue(MotionEvent.AXIS_HAT_X))
            hat(d, 8, 9, hatY, event.getAxisValue(MotionEvent.AXIS_HAT_Y))
            return true
        }
        return super.dispatchGenericMotionEvent(event)
    }

    companion object {
        init {
            System.loadLibrary("jumpman_game")
        }
    }
}
