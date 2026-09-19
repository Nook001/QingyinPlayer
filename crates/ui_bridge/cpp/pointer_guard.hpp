#pragma once

extern "C" {
void qingyin_install_pointer_guard();
void qingyin_drop_pointer_grabs_later();
int qingyin_drop_pointer_grabs(char *out, int out_len);
}
