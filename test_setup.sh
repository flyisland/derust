#!/bin/bash
rm -rf /tmp/testdir
mkdir -p /tmp/testdir
cd /tmp/testdir
dd if=/dev/urandom of=file1.txt bs=1M count=10
dd if=/dev/urandom of=file2.txt bs=1M count=10
dd if=/dev/urandom of=file3.txt bs=1M count=5
dd if=/dev/urandom of=file4.txt bs=1M count=5
cp file1.txt copy1.txt
mkdir dir01
cp file3.txt dir01/copy3.txt
ln file1.txt hardlink1.txt
ln copy1.txt hardlink_copy1.txt
ln file2.txt dir01/hardlink2.txt
ln -s file3.txt softlink3-1.txt
ln -s file3.txt softlink3-2.txt
ln -s file4.txt softlink4.txt
ln -s ~/Pictures softlink5.txt
rm file4.txt
touch z1.txt
touch z2.txt