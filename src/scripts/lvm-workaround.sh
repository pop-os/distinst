#!/bin/sh
#
# The LVM autodetect in Ubuntu has been broken since 2007, but I
# found this simple workaround which solves the problem.
#
# Source: https://askubuntu.com/a/834626/98752

PREREQ=""
prereqs()
{
   echo "$PREREQ"
}
case $1 in
prereqs)
   prereqs
   exit 0
   ;;
esac
. /scripts/functions

# The main line of code in this initramfs script
lvm vgchange -ay