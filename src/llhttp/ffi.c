#include <stdint.h>
#include <stdlib.h>
#include "llhttp.h"



llhttp_settings_t* llhttp_settings_new()
{
    llhttp_settings_t *ptr = malloc(sizeof(llhttp_settings_t));
    llhttp_settings_init(ptr);
    return ptr;   
}

void llhttp_settings_free(llhttp_settings_t* ptr)
{
    free(ptr);
}
