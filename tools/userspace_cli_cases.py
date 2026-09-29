"""Additional real-guest assertions, called by the framebuffer/RFB harness."""

from collections.abc import Callable


def check_cli(command: Callable[[str, str], str]) -> None:
    def success(line: str, expected: str = "") -> str:
        reply = command(line, expected)
        if "sh: " in reply:
            raise AssertionError(f"command failed in guest: {line!r}: {reply!r}")
        return reply

    success("echo \"Shift < > !\"", "\nShift < > !\n")
    command("cat /dev/zero", "operation requires a regular file")
    success("help grep", "Usage: grep")
    success("man grep", "\nSYNOPSIS\n")
    success("man shell", "No pipes, append")
    success("apropos directory", "pwd")
    success("which cat type", "type: shell builtin which")
    success("vibrix status", "Vibrix status")
    success("vibrix status", "processes: total=")
    success("vibrix status", "root: bootstrap RAM mounted (volatile)")
    success("vibrix doctor", "doctor: PASS (bootstrap checks only")
    success("write '/tmp/my note' 'hello world'")
    success("touch '/tmp/my note'")
    success("cat '/tmp/my note'", "\nhello world\n")
    success("echo 'second line' > /tmp/other")
    success("cat < /tmp/other", "\nsecond line\n")
    success("cat '/tmp/my note' /tmp/other > /tmp/lines")
    success("head -n 1 /tmp/lines", "\nhello world\n")
    success("tail -n 1 /tmp/lines", "\nsecond line\n")
    success("wc -l /tmp/lines", "\n2\n")
    success("grep -n second /tmp/lines", "\n2:second line\n")
    success("sort -r /tmp/lines", "\nsecond line\nhello world\n")
    success("uniq -c /tmp/lines", "\n1 hello world\n1 second line\n")
    success("nl /tmp/lines", "\n1\thello world\n2\tsecond line\n")
    success("hexdump /tmp/other", "00000000: 73 65 63 6f 6e 64")
    success("basename /tmp/note.txt .txt", "\nnote\n")
    success("dirname /tmp/note.txt", "\n/tmp\n")
    command("cp /tmp/lines /tmp/./lines", "source and destination are the same file")
    command("cat < /tmp/lines > /tmp/lines", "input and output refer to the same path")
    command("echo no >> /tmp/lines", "invalid or repeated redirection")
    success("wc -l /tmp/lines", "\n2\n")
    command("grep absent /tmp/lines", "\nvibrix$ ")
    success("status", "\n1\n")
    success("false")
    success("status", "\n1\n")
    success("true")
    success("status", "\n0\n")
    success("mkdir /tmp/empty")
    command("rm /tmp/empty", "is a directory; use rmdir")
    success("rmdir /tmp/empty")
    success("history", "history")
    # Five copies exceed the old 256-byte per-file bootstrap ceiling.
    welcome = "Vibrix bootstrap filesystem: files live in RAM until reboot.\n"
    success("cat /welcome /welcome /welcome /welcome /welcome > /tmp/larger")
    success("wc -c /tmp/larger", f"\n{5 * len(welcome)}\n")
    names = " ".join(f"/tmp/n{index}" for index in range(12))
    success("touch " + names)
    success("rm " + names)
