//! The system calls an application may not use, as a seccomp filter: a
//! small program for the kernel (classic BPF) that bubblewrap installs
//! before starting the application (`--seccomp <fd>`). Written by hand, so
//! no seccomp library is needed.
//!
//! Refused (EPERM): loading kernel modules, kexec, mounting, keyrings,
//! ptrace, BPF and perf, namespaces (unshare, setns, clone with
//! CLONE_NEWUSER), changing the clock, swap, reboot, typing into the
//! terminal (TIOCSTI) and the like. clone3 answers "not supported", so the
//! C library falls back to clone, whose flags the filter can see. Programs
//! of another architecture (x32, i386) are stopped.

/// One instruction of a classic BPF program (struct sock_filter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Insn {
    pub code: u16,
    pub jt: u8,
    pub jf: u8,
    pub k: u32,
}

const LD_W_ABS: u16 = 0x20; // load a 32-bit word of seccomp_data
const JEQ: u16 = 0x15; // jump if equal
const JGE: u16 = 0x35; // jump if greater or equal
const JSET: u16 = 0x45; // jump if any of the bits are set
const RET: u16 = 0x06;

const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
const X32_BIT: u32 = 0x4000_0000;
const RET_ALLOW: u32 = 0x7fff_0000;
const RET_KILL_PROCESS: u32 = 0x8000_0000;
const RET_ERRNO: u32 = 0x0005_0000;
const EPERM: u32 = 1;
const ENOSYS: u32 = 38;

// Offsets in struct seccomp_data: nr, arch, instruction pointer, args[6].
const OFF_NR: u32 = 0;
const OFF_ARCH: u32 = 4;
const fn off_arg_low(i: u32) -> u32 {
    16 + 8 * i
}

const SYS_IOCTL: u32 = 16;
const SYS_CLONE: u32 = 56;
const SYS_CLONE3: u32 = 435;
const TIOCSTI: u32 = 0x5412;
const TIOCLINUX: u32 = 0x541c;
const CLONE_NEWUSER: u32 = 0x1000_0000;

/// x86-64 system call numbers refused with EPERM.
pub const DENIED: &[(u32, &str)] = &[
    (101, "ptrace"),
    (103, "syslog"),
    (134, "uselib"),
    (153, "vhangup"),
    (155, "pivot_root"),
    (159, "adjtimex"),
    (163, "acct"),
    (164, "settimeofday"),
    (165, "mount"),
    (166, "umount2"),
    (167, "swapon"),
    (168, "swapoff"),
    (169, "reboot"),
    (170, "sethostname"),
    (171, "setdomainname"),
    (172, "iopl"),
    (173, "ioperm"),
    (175, "init_module"),
    (176, "delete_module"),
    (179, "quotactl"),
    (180, "nfsservctl"),
    (212, "lookup_dcookie"),
    (227, "clock_settime"),
    (246, "kexec_load"),
    (248, "add_key"),
    (249, "request_key"),
    (250, "keyctl"),
    (272, "unshare"),
    (298, "perf_event_open"),
    (304, "open_by_handle_at"),
    (305, "clock_adjtime"),
    (308, "setns"),
    (313, "finit_module"),
    (320, "kexec_file_load"),
    (321, "bpf"),
    (323, "userfaultfd"),
    (428, "open_tree"),
    (429, "move_mount"),
    (430, "fsopen"),
    (431, "fsconfig"),
    (432, "fsmount"),
    (433, "fspick"),
    (442, "mount_setattr"),
];

fn ld(off: u32) -> Insn {
    Insn {
        code: LD_W_ABS,
        jt: 0,
        jf: 0,
        k: off,
    }
}
fn jmp(code: u16, k: u32, jt: u8, jf: u8) -> Insn {
    Insn { code, jt, jf, k }
}
fn ret(k: u32) -> Insn {
    Insn {
        code: RET,
        jt: 0,
        jf: 0,
        k,
    }
}

/// The filter program.
pub fn program() -> Vec<Insn> {
    let mut p = vec![
        // Only x86-64 programs.
        ld(OFF_ARCH),
        jmp(JEQ, AUDIT_ARCH_X86_64, 1, 0),
        ret(RET_KILL_PROCESS),
        ld(OFF_NR),
        // x32 system calls (numbers with bit 30 set): stopped.
        jmp(JGE, X32_BIT, 0, 1),
        ret(RET_KILL_PROCESS),
        jmp(JEQ, SYS_CLONE3, 0, 1),
        ret(RET_ERRNO | ENOSYS),
    ];
    for &(nr, _) in DENIED {
        p.push(jmp(JEQ, nr, 0, 1));
        p.push(ret(RET_ERRNO | EPERM));
    }
    // ioctl: refuse TIOCSTI and TIOCLINUX (request in the second argument).
    p.extend([
        jmp(JEQ, SYS_IOCTL, 0, 5),
        ld(off_arg_low(1)),
        jmp(JEQ, TIOCSTI, 2, 0),
        jmp(JEQ, TIOCLINUX, 1, 0),
        ret(RET_ALLOW),
        ret(RET_ERRNO | EPERM),
    ]);
    // clone: refuse a new user namespace (flags in the first argument).
    p.extend([
        jmp(JEQ, SYS_CLONE, 0, 3),
        ld(off_arg_low(0)),
        jmp(JSET, CLONE_NEWUSER, 0, 1),
        ret(RET_ERRNO | EPERM),
        ret(RET_ALLOW),
    ]);
    p
}

/// The program as the bytes bubblewrap reads (struct sock_filter[]).
pub fn bytes() -> Vec<u8> {
    let mut out = Vec::new();
    for i in program() {
        out.extend_from_slice(&i.code.to_le_bytes());
        out.push(i.jt);
        out.push(i.jf);
        out.extend_from_slice(&i.k.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny BPF interpreter, enough to run the filter in the tests.
    fn eval(nr: u32, arch: u32, args: [u64; 6]) -> u32 {
        let p = program();
        let mut data = Vec::new();
        data.extend_from_slice(&nr.to_le_bytes());
        data.extend_from_slice(&arch.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        for a in args {
            data.extend_from_slice(&a.to_le_bytes());
        }
        let (mut pc, mut acc) = (0usize, 0u32);
        loop {
            let i = p[pc];
            pc += 1;
            match i.code {
                LD_W_ABS => {
                    let o = i.k as usize;
                    acc = u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
                }
                RET => return i.k,
                JEQ | JGE | JSET => {
                    let taken = match i.code {
                        JEQ => acc == i.k,
                        JGE => acc >= i.k,
                        _ => acc & i.k != 0,
                    };
                    pc += if taken { i.jt } else { i.jf } as usize;
                }
                _ => panic!("unknown instruction"),
            }
        }
    }

    const A: u32 = AUDIT_ARCH_X86_64;

    #[test]
    fn decisions() {
        assert_eq!(eval(0, A, [0; 6]), RET_ALLOW); // read
        assert_eq!(eval(101, A, [0; 6]), RET_ERRNO | EPERM); // ptrace
        assert_eq!(eval(165, A, [0; 6]), RET_ERRNO | EPERM); // mount
        assert_eq!(eval(435, A, [0; 6]), RET_ERRNO | ENOSYS); // clone3
        assert_eq!(eval(56, A, [0x11, 0, 0, 0, 0, 0]), RET_ALLOW); // clone (fork)
        assert_eq!(
            eval(56, A, [CLONE_NEWUSER as u64, 0, 0, 0, 0, 0]),
            RET_ERRNO | EPERM
        );
        assert_eq!(
            eval(16, A, [1, TIOCSTI as u64, 0, 0, 0, 0]),
            RET_ERRNO | EPERM
        );
        assert_eq!(eval(16, A, [1, 0x5401, 0, 0, 0, 0]), RET_ALLOW); // TCGETS
        assert_eq!(eval(1, 0x4000_0003, [0; 6]), RET_KILL_PROCESS); // i386
        assert_eq!(eval(X32_BIT | 1, A, [0; 6]), RET_KILL_PROCESS); // x32
        for &(nr, name) in DENIED {
            assert_eq!(eval(nr, A, [0; 6]), RET_ERRNO | EPERM, "{name}");
        }
    }

    #[test]
    fn encoding() {
        assert_eq!(bytes().len(), program().len() * 8);
        assert!(program().len() < 4096);
    }
}
