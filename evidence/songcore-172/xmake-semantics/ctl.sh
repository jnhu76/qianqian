#!/bin/bash
false
echo "ctl_false=$?"
export PATH=/home/hoo/.local/bin:$PATH
cd /home/hoo/tmp/sem5
xmake build tsyntaxerr > /dev/null 2>&1
echo "syntax_exit=$?"
xmake build tgood > /dev/null 2>&1
echo "good_exit=$?"
xmake l /home/hoo/tmp/e1.lua > /dev/null 2>&1
echo "lua_exit1=$?"
B64DONE=1
