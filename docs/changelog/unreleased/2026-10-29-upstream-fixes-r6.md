## From upstream

- **Fixes ported from the weekly upstream review:** DNG `WhiteLevel` written as LONG for float DNGs, Canon CR2 slice geometry validated, DNG opcode byte counts checked without overflow, face index caches over 1 GiB read, signed numbers refused in URL ports, `Content-Range` and `Content-Length`, ZIP entries with inconsistent sizes refused, EXIF time zone offsets limited to 18 hours, and a library lock removes only the note it wrote. (no fork PR)
