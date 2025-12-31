#!/bin/bash
# Test script to run gwbasic command

# Create expect script
cat > /tmp/test_gwbasic.exp <<'EOF'
#!/usr/bin/expect -f
set timeout 30
spawn ./scripts/boot_test.sh -i
expect "Login:" {
    send "test\r"
}
expect "Password:" {
    send "test\r"
}
expect "$" {
    send "gwbasic\r"
}
expect {
    timeout { puts "TIMEOUT"; exit 1 }
    eof { puts "EOF"; exit 0 }
}
EOF

chmod +x /tmp/test_gwbasic.exp

# Run with serial output to stdout
if command -v expect >/dev/null 2>&1; then
    /tmp/test_gwbasic.exp
else
    echo "expect not installed, running manually..."
    timeout 30 ./scripts/boot_test.sh 2>&1
fi
