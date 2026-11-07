//! The few three.js matrix routines `movingPlatform.ts` goes through (`Euler` -> `Quaternion` ->
//! `Matrix4.compose`, the parent multiply of `updateMatrixWorld`, `Matrix4.invert`,
//! `Vector3.applyMatrix4`), ported operation for operation so the rounding of every float matches
//! the TypeScript bit for bit (glam's formulas round differently).
use jumpman_sim::js_math::{js_cos, js_sin};

/// Column-major 4x4 like `Matrix4.elements`.
#[derive(Clone, Copy, Debug)]
pub struct Mat4(pub [f64; 16]);

/// `Quaternion.setFromEuler` for order 'YXZ' (x, y, z, w).
pub fn quat_from_euler_yxz(x: f64, y: f64, z: f64) -> [f64; 4] {
    let (c1, c2, c3) = (js_cos(x / 2.0), js_cos(y / 2.0), js_cos(z / 2.0));
    let (s1, s2, s3) = (js_sin(x / 2.0), js_sin(y / 2.0), js_sin(z / 2.0));
    [
        s1 * c2 * c3 + c1 * s2 * s3,
        c1 * s2 * c3 - s1 * c2 * s3,
        c1 * c2 * s3 - s1 * s2 * c3,
        c1 * c2 * c3 + s1 * s2 * s3,
    ]
}

/// `Vector3.applyEuler` for an 'YXZ' Euler built from s16 angles (`shipAngles` in `sm64World.ts`):
/// quaternion from the Euler, then `applyQuaternion`.
pub fn apply_euler_yxz(v: [f64; 3], pitch: f64, yaw: f64, roll: f64) -> [f64; 3] {
    let [qx, qy, qz, qw] = quat_from_euler_yxz(pitch * std::f64::consts::PI / 32768.0, yaw * std::f64::consts::PI / 32768.0, roll * std::f64::consts::PI / 32768.0);
    let (vx, vy, vz) = (v[0], v[1], v[2]);
    let tx = 2.0 * (qy * vz - qz * vy);
    let ty = 2.0 * (qz * vx - qx * vz);
    let tz = 2.0 * (qx * vy - qy * vx);
    [vx + qw * tx + qy * tz - qz * ty, vy + qw * ty + qz * tx - qx * tz, vz + qw * tz + qx * ty - qy * tx]
}

impl Mat4 {
    /// `Matrix4.compose(position, quaternion, scale)`.
    pub fn compose(position: [f64; 3], q: [f64; 4], scale: [f64; 3]) -> Self {
        let [x, y, z, w] = q;
        let (x2, y2, z2) = (x + x, y + y, z + z);
        let (xx, xy, xz) = (x * x2, x * y2, x * z2);
        let (yy, yz, zz) = (y * y2, y * z2, z * z2);
        let (wx, wy, wz) = (w * x2, w * y2, w * z2);
        let [sx, sy, sz] = scale;
        Mat4([
            (1.0 - (yy + zz)) * sx,
            (xy + wz) * sx,
            (xz - wy) * sx,
            0.0,
            (xy - wz) * sy,
            (1.0 - (xx + zz)) * sy,
            (yz + wx) * sy,
            0.0,
            (xz + wy) * sz,
            (yz - wx) * sz,
            (1.0 - (xx + yy)) * sz,
            0.0,
            position[0],
            position[1],
            position[2],
            1.0,
        ])
    }

    /// `Matrix4.multiplyMatrices(a, b)`.
    pub fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
        let (ae, be) = (&a.0, &b.0);
        let (a11, a12, a13, a14) = (ae[0], ae[4], ae[8], ae[12]);
        let (a21, a22, a23, a24) = (ae[1], ae[5], ae[9], ae[13]);
        let (a31, a32, a33, a34) = (ae[2], ae[6], ae[10], ae[14]);
        let (a41, a42, a43, a44) = (ae[3], ae[7], ae[11], ae[15]);
        let (b11, b12, b13, b14) = (be[0], be[4], be[8], be[12]);
        let (b21, b22, b23, b24) = (be[1], be[5], be[9], be[13]);
        let (b31, b32, b33, b34) = (be[2], be[6], be[10], be[14]);
        let (b41, b42, b43, b44) = (be[3], be[7], be[11], be[15]);
        let mut te = [0.0; 16];
        te[0] = a11 * b11 + a12 * b21 + a13 * b31 + a14 * b41;
        te[4] = a11 * b12 + a12 * b22 + a13 * b32 + a14 * b42;
        te[8] = a11 * b13 + a12 * b23 + a13 * b33 + a14 * b43;
        te[12] = a11 * b14 + a12 * b24 + a13 * b34 + a14 * b44;
        te[1] = a21 * b11 + a22 * b21 + a23 * b31 + a24 * b41;
        te[5] = a21 * b12 + a22 * b22 + a23 * b32 + a24 * b42;
        te[9] = a21 * b13 + a22 * b23 + a23 * b33 + a24 * b43;
        te[13] = a21 * b14 + a22 * b24 + a23 * b34 + a24 * b44;
        te[2] = a31 * b11 + a32 * b21 + a33 * b31 + a34 * b41;
        te[6] = a31 * b12 + a32 * b22 + a33 * b32 + a34 * b42;
        te[10] = a31 * b13 + a32 * b23 + a33 * b33 + a34 * b43;
        te[14] = a31 * b14 + a32 * b24 + a33 * b34 + a34 * b44;
        te[3] = a41 * b11 + a42 * b21 + a43 * b31 + a44 * b41;
        te[7] = a41 * b12 + a42 * b22 + a43 * b32 + a44 * b42;
        te[11] = a41 * b13 + a42 * b23 + a43 * b33 + a44 * b43;
        te[15] = a41 * b14 + a42 * b24 + a43 * b34 + a44 * b44;
        Mat4(te)
    }

    pub const IDENTITY: Mat4 = Mat4([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);

    /// `Matrix4.invert()` (the gl-matrix formulation three.js uses).
    pub fn invert(&self) -> Mat4 {
        let te = &self.0;
        let (n11, n21, n31, n41) = (te[0], te[1], te[2], te[3]);
        let (n12, n22, n32, n42) = (te[4], te[5], te[6], te[7]);
        let (n13, n23, n33, n43) = (te[8], te[9], te[10], te[11]);
        let (n14, n24, n34, n44) = (te[12], te[13], te[14], te[15]);
        let t1 = n11 * n22 - n21 * n12;
        let t2 = n11 * n32 - n31 * n12;
        let t3 = n11 * n42 - n41 * n12;
        let t4 = n21 * n32 - n31 * n22;
        let t5 = n21 * n42 - n41 * n22;
        let t6 = n31 * n42 - n41 * n32;
        let t7 = n13 * n24 - n23 * n14;
        let t8 = n13 * n34 - n33 * n14;
        let t9 = n13 * n44 - n43 * n14;
        let t10 = n23 * n34 - n33 * n24;
        let t11 = n23 * n44 - n43 * n24;
        let t12 = n33 * n44 - n43 * n34;
        let det = t1 * t12 - t2 * t11 + t3 * t10 + t4 * t9 - t5 * t8 + t6 * t7;
        if det == 0.0 {
            return Mat4([0.0; 16]);
        }
        let d = 1.0 / det;
        Mat4([
            (n22 * t12 - n32 * t11 + n42 * t10) * d,
            (n31 * t11 - n21 * t12 - n41 * t10) * d,
            (n24 * t6 - n34 * t5 + n44 * t4) * d,
            (n33 * t5 - n23 * t6 - n43 * t4) * d,
            (n32 * t9 - n12 * t12 - n42 * t8) * d,
            (n11 * t12 - n31 * t9 + n41 * t8) * d,
            (n34 * t3 - n14 * t6 - n44 * t2) * d,
            (n13 * t6 - n33 * t3 + n43 * t2) * d,
            (n12 * t11 - n22 * t9 + n42 * t7) * d,
            (n21 * t9 - n11 * t11 - n41 * t7) * d,
            (n14 * t5 - n24 * t3 + n44 * t1) * d,
            (n23 * t3 - n13 * t5 - n43 * t1) * d,
            (n22 * t8 - n12 * t10 - n32 * t7) * d,
            (n11 * t10 - n21 * t8 + n31 * t7) * d,
            (n24 * t2 - n14 * t4 - n34 * t1) * d,
            (n13 * t4 - n23 * t2 + n33 * t1) * d,
        ])
    }

    /// `Vector3.applyMatrix4`.
    pub fn apply(&self, v: [f64; 3]) -> [f64; 3] {
        let (x, y, z) = (v[0], v[1], v[2]);
        let e = &self.0;
        let w = 1.0 / (e[3] * x + e[7] * y + e[11] * z + e[15]);
        [(e[0] * x + e[4] * y + e[8] * z + e[12]) * w, (e[1] * x + e[5] * y + e[9] * z + e[13]) * w, (e[2] * x + e[6] * y + e[10] * z + e[14]) * w]
    }
}
