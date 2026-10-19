/* Userspace stand-ins for the kernel APIs r9t_fan.c uses: an in-memory EC with
 * injectable faults and a fake workqueue. Used by crates/r9t-hal/tests/r9t_fan_c_faults.rs. */
#include <stdio.h>
#include <stdint.h>
#include <stdbool.h>
#include <string.h>
#include <errno.h>
#include <stdlib.h>
typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
struct work_struct {int unused;};
struct delayed_work {struct work_struct work;};
struct kernel_param {int unused;};
static int rpm_value[2]={3200,3200};
struct kernel_param_ops {int (*set)(const char *,const struct kernel_param *);int (*get)(char *,const struct kernel_param *);};
struct notifier_block {int (*notifier_call)(struct notifier_block *,unsigned long,void *);};
static unsigned long jiffies;
static int queued, pwm_writes, reads, writes, fail_read, fail_write, fail_auto, stuck_bit, logs;
static u8 ec[0x2000];
#define DEFINE_MUTEX(x) int x
#define DECLARE_DELAYED_WORK(x,f) struct delayed_work x
#define mutex_lock(x) ((void)(x))
#define mutex_unlock(x) ((void)(x))
#define system_dfl_wq NULL
#define msecs_to_jiffies(x) (x)
#define jiffies_to_msecs(x) ((unsigned int)(x))
#define time_after_eq(a,b) ((long)((a)-(b))>=0)
#define time_before(a,b) ((long)((a)-(b))<0)
#define mod_delayed_work(q,w,t) (queued=1)
#define cancel_delayed_work_sync(w) (queued=0)
#define sysfs_streq(a,b) (!strcmp(a,b))
#define scnprintf snprintf
#define PAGE_SIZE 4096
#define PM_SUSPEND_PREPARE 1
#define PM_HIBERNATION_PREPARE 2
#define PM_RESTORE_PREPARE 3
#define PM_POST_SUSPEND 4
#define PM_POST_HIBERNATION 5
#define PM_POST_RESTORE 6
#define NOTIFY_BAD 0x8002
#define NOTIFY_OK 1
#define DMI_SYS_VENDOR 1
#define DMI_PRODUCT_NAME 2
#define dmi_match(a,b) 1
#define register_pm_notifier(x) 0
#define unregister_pm_notifier(x) ((void)(x))
#define pr_err(...) (logs++)
#define msleep(x) ((void)(x))
#define __init
#define __exit
#define module_param_cb(...)
#define module_init(...)
#define module_exit(...)
#define MODULE_LICENSE(...)
#define MODULE_DESCRIPTION(...)
#define MODULE_SOFTDEP(...)
#define MODULE_VERSION(...)
#define MODULE_PARM_DESC(...)
#define pr_warn(...) (logs++)
typedef unsigned short umode_t;
struct device {int unused;};
struct platform_device {struct device dev;};
static struct platform_device fake_pdev;
static struct device fake_hwmon;
static int fail_hwmon;
#define PLATFORM_DEVID_NONE (-1)
#define IS_ERR(p) ((unsigned long)(p)>=(unsigned long)-4095)
#define PTR_ERR(p) ((long)(p))
#define ERR_PTR(e) ((void *)(long)(e))
static struct platform_device *platform_device_register_simple(const char *n,int id,void *r,int c){(void)n;(void)id;(void)r;(void)c;return &fake_pdev;}
#define platform_device_unregister(p) ((void)(p))
enum hwmon_sensor_types {hwmon_temp,hwmon_fan,hwmon_pwm};
enum {hwmon_temp_input,hwmon_fan_input=0,hwmon_pwm_input=0,hwmon_temp_label=1,hwmon_fan_label=1};
#define HWMON_F_INPUT 1
#define HWMON_F_LABEL 2
#define HWMON_T_INPUT 1
#define HWMON_T_LABEL 2
#define HWMON_PWM_INPUT 1
#define HWMON_CHANNEL_INFO(...) NULL
struct hwmon_channel_info {int unused;};
struct hwmon_ops {umode_t visible;int (*read)(struct device *,enum hwmon_sensor_types,u32,int,long *);int (*read_string)(struct device *,enum hwmon_sensor_types,u32,int,const char **);};
struct hwmon_chip_info {const struct hwmon_ops *ops;const struct hwmon_channel_info *const *info;};
static struct device *hwmon_device_register_with_info(struct device *d,const char *n,void *p,const struct hwmon_chip_info *c,void *g){(void)d;(void)n;(void)p;(void)c;(void)g;return fail_hwmon?ERR_PTR(-ENOMEM):&fake_hwmon;}
#define hwmon_device_unregister(d) ((void)(d))
static int kstrtouint(const char *s,unsigned int base,unsigned int *out){char *end;unsigned long v=strtoul(s,&end,base);if(end==s||(*end&&*end!='\n'))return -EINVAL;*out=(unsigned int)v;return 0;}
static int kstrtobool(const char *s,bool *out){if(*s=='1'||*s=='Y'||*s=='y'){*out=true;return 0;}if(*s=='0'||*s=='N'||*s=='n'){*out=false;return 0;}return -EINVAL;}
static int param_get_uint(char *b,const struct kernel_param *kp){(void)b;(void)kp;return 0;}
int uniwill_read_ec_ram(u16 addr,u8 *out){reads++;if(fail_read)return -EAGAIN;
 if(addr==0x0464||addr==0x0465||addr==0x046c||addr==0x046d){int v=rpm_value[addr>=0x046c];*out=(addr&1)?(u8)(v&0xff):(u8)(v>>8);return 0;}
 *out=ec[addr];return 0;}
int uniwill_write_ec_ram(u16 addr,u8 value){writes++;if(addr==0x1804||addr==0x1809)pwm_writes++;if(fail_write)return -EIO;if(!(stuck_bit&&addr==0x0751))ec[addr]=value;return 0;}
u32 uw_set_fan_auto(void){return (u32)fail_auto;}
