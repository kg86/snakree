# Scenario silesia-quick: every benchmark scenario on Silesia, quick and rough.
# It covers build, slide, search and memory, with several pattern lengths.
# One repetition is used on purpose, so it is for trying out the experiment process, not for accurate numbers.
description="Silesia, 12 files cut to 4 MiB, windows 64 KiB and 1 MiB, pattern lengths 1 to 64, one repetition"
files="dickens mozilla mr nci ooffice osdb reymont samba sao webster xml x-ray"
bench_args="--no-synthetic --limit 4194304 --build-limit 4194304 --window 65536 --window 1048576 --reps 1 --pattern-len 1 --pattern-len 2 --pattern-len 4 --pattern-len 8 --pattern-len 16 --pattern-len 32 --pattern-len 64"
