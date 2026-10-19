// SPDX-License-Identifier: GPL-2.0
/* Restricted R9T fan bridge. All writes are fixed fan registers only.
 * 0.3.0: EC readings are cached (status, status2 and hwmon read the cache, refreshed
 * outside the control mutex), the thermal override has a release threshold and hold
 * time, and duty below 50 % is possible only through min_duty (measured floor) or the
 * test-only allow_low_duty, with in-kernel stall detection. Defaults behave as 0.2.0. */
#include <linux/module.h>
#include <linux/dmi.h>
#include <linux/kernel.h>
#include <linux/workqueue.h>
#include <linux/mutex.h>
#include <linux/suspend.h>
#include <linux/delay.h>
#include <linux/err.h>
#include <linux/platform_device.h>
#include <linux/hwmon.h>
extern int uniwill_read_ec_ram(u16,u8 *);
extern int uniwill_write_ec_ram(u16,u8);
extern u32 uw_set_fan_auto(void);
static DEFINE_MUTEX(lock);         /* control state and every EC write */
static DEFINE_MUTEX(refresh_lock); /* one snapshot refresh at a time; never taken under `lock` */
static DEFINE_MUTEX(snap_lock);    /* short copies of `snap` */
static int mode; /* 0 auto, 1 manual, 2 full */
static unsigned int targets[2]={100,100};
static unsigned long deadline;
static int last_error;
static bool thermal_override;
static unsigned long override_last; /* last sample at or above the engage threshold */
static bool auto_pending;
static bool suspended;
static bool stopping;
static bool allow_low_duty;          /* test only: manual down to 20 % */
static unsigned int min_duty=50;     /* measured safe floor; 50 until the low-duty sweep */
static unsigned int cache_ms=1000;
static u8 held[2];                   /* floor temperature with 3 °C hysteresis */
static unsigned int stall_ticks,stalls;
struct snapshot{u8 cpu,gpu,reg,pwm0,pwm1;int r0,r1;unsigned long at;bool valid;};
static struct snapshot snap;
static unsigned int snap_gen;
static struct platform_device *pdev;
static struct device *hwmon;
static void worker(struct work_struct *);
static DECLARE_DELAYED_WORK(work,worker);
static int read_reg(u16 addr,u8 *value){return uniwill_read_ec_ram(addr,value);}
/* EC state changed under us: the next reader refreshes, a refresh in flight is dropped. */
static void invalidate(void){mutex_lock(&snap_lock);snap.valid=false;snap_gen++;mutex_unlock(&snap_lock);}
/* Keep the lease state until the EC confirms firmware control. A failed
 * fallback must never resume manual PWM writes or lose its retry worker. */
static int automatic(void){
 u8 reg;int ret;
 auto_pending=true;
 invalidate();
 ret=(int)uw_set_fan_auto();
 if(!ret)ret=read_reg(0x0751,&reg);
 if(!ret)ret=uniwill_write_ec_ram(0x0751,reg&~0x40);
 if(!ret)ret=read_reg(0x0751,&reg);
 if(!ret&&(reg&0x40))ret=-EIO;
 if(ret){last_error=ret;return ret;}
 mode=0;thermal_override=false;auto_pending=false;stall_ticks=0;
 return 0;
}
static void queue_retry(void){
 if(!stopping&&!suspended&&(mode||auto_pending))
  mod_delayed_work(system_dfl_wq,&work,msecs_to_jiffies(1000)); /* unbound; system_wq is deprecated on 7.2 */
}
static int temperatures(u8 *cpu,u8 *gpu){
 int ret=read_reg(0x043e,cpu);
 if(!ret)ret=read_reg(0x044f,gpu);
 if(!ret&&(*cpu<1||*cpu>115||*gpu<1||*gpu>115))ret=-ERANGE;
 return ret;
}
static int rpm(u16 addr,int *value){
 u8 high,low,check;int ret,i;
 for(i=0;i<3;i++){
  ret=read_reg(addr,&high);if(ret)return ret;
  ret=read_reg(addr+1,&low);if(ret)return ret;
  ret=read_reg(addr,&check);if(ret)return ret;
  if(high==check){*value=(high<<8)|low;return *value<=12000?0:-ERANGE;}
 }
 return -EAGAIN;
}
/* Engage at 95/87 °C, release below 90/82 °C once 10 s have passed without a hot sample. */
static void thermal_update(u8 cpu,u8 gpu){
 if(cpu>=95||gpu>=87){thermal_override=true;override_last=jiffies;}
 else if(thermal_override&&cpu<90&&gpu<82&&time_after_eq(jiffies,override_last+msecs_to_jiffies(10000)))
  thermal_override=false;
}
/* Lowest duty for a fan: the base rises to 50 % with temperature. min_duty ramps over
 * CPU 60→75 °C / GPU 55→70 °C; the allow_low_duty test base (20 %) over CPU 70→80 °C /
 * GPU 65→75 °C, the sweep's stop thresholds, since the machine idles near 66 °C. Never
 * above 50 %, so with min_duty=50 every command behaves as on 0.2.0. */
static unsigned int floor_value(int fan){
 unsigned int base=allow_low_duty?20:min_duty,start,span;
 if(allow_low_duty){start=fan?65:70;span=10;}else{start=fan?55:60;span=15;}
 if(base>=50||held[fan]<=start)return base;
 if(held[fan]>=start+span)return 50;
 return base+(50-base)*(held[fan]-start)/span;
}
static unsigned int floor_for(int fan,u8 temp){
 if(temp>held[fan]||temp+3<=held[fan])held[fan]=temp;
 return floor_value(fan);
}
static unsigned int duty(int fan,u8 temp){
 unsigned int f=floor_for(fan,temp);
 if(mode==2||thermal_override)return 100;
 return targets[fan]>f?targets[fan]:f;
}
/* Below 50 % a fan may stop: two ticks under 500 rpm hand control back to firmware. */
static int stall_check(unsigned int d0,unsigned int d1){
 int r0=0,r1=0,ret=0;
 if(d0>=50&&d1>=50){stall_ticks=0;return 0;}
 if(d0<50)ret=rpm(0x0464,&r0);
 if(!ret&&d1<50)ret=rpm(0x046c,&r1);
 if(ret)return ret;
 if((d0<50&&r0<500)||(d1<50&&r1<500))stall_ticks++;else stall_ticks=0;
 if(stall_ticks<2)return 0;
 stalls++;stall_ticks=0;
 return -EPIPE;
}
static int update(void){
 u8 reg,cpu,gpu;int ret;unsigned int d0,d1;
 ret=temperatures(&cpu,&gpu);
 if(ret)return ret;
 thermal_update(cpu,gpu);
 ret=read_reg(0x0751,&reg);
 if(ret)return ret;
 if(!(reg&0x40))return -ECANCELED; /* Firmware/hotkey changed the mode. */
 if(mode){
  d0=duty(0,cpu);d1=duty(1,gpu);
  ret=uniwill_write_ec_ram(0x1804,d0*2);
  if(!ret)ret=uniwill_write_ec_ram(0x1809,d1*2);
  if(!ret&&mode==1)ret=stall_check(d0,d1);
 }
 return ret;
}
static void worker(struct work_struct *unused){
 int ret=0;
 mutex_lock(&lock);
 if(stopping||suspended)goto out;
 if(auto_pending)automatic();
 else if(mode){
  if(time_after_eq(jiffies,deadline)){last_error=-ETIMEDOUT;automatic();}
  else{
   ret=update();
   if(ret){last_error=ret;automatic();}
  }
 }
 queue_retry();
out:
 mutex_unlock(&lock);
}
static int control_set(const char *value,const struct kernel_param *kp){
 unsigned int cpu=100,gpu=100,low;int wanted=0,ret=0;u8 reg,t0,t1;char extra;
 mutex_lock(&lock);
 low=allow_low_duty?20:min_duty;
 if(sysfs_streq(value,"auto"))wanted=0;
 else if(sysfs_streq(value,"boost"))wanted=2;
 else if(sscanf(value,"manual %u %u %c",&cpu,&gpu,&extra)==2&&cpu>=low&&cpu<=100&&gpu>=low&&gpu<=100)wanted=1;
 else{ret=-EINVAL;goto out;}
 if(stopping||suspended){ret=-EBUSY;goto out;}
 if(!wanted){ret=automatic();queue_retry();goto out;}
 if(auto_pending){ret=-EBUSY;goto out;}
 ret=temperatures(&t0,&t1);
 if(ret)goto out;
 ret=read_reg(0x0751,&reg);
 if(ret)goto out;
 invalidate();
 if(!(reg&0x40))ret=uniwill_write_ec_ram(0x0751,reg|0x40);
 if(ret)goto out;
 mode=wanted;targets[0]=cpu;targets[1]=gpu;deadline=jiffies+msecs_to_jiffies(15000);last_error=0;stall_ticks=0;
 ret=update();
 if(ret){last_error=ret;automatic();}
 queue_retry();
out:
 mutex_unlock(&lock);
 return ret;
}
static int refresh(struct snapshot *out){
 struct snapshot s;unsigned int gen;int ret;
 mutex_lock(&snap_lock);gen=snap_gen;mutex_unlock(&snap_lock);
 ret=temperatures(&s.cpu,&s.gpu);
 if(!ret)ret=rpm(0x0464,&s.r0);
 if(!ret)ret=rpm(0x046c,&s.r1);
 if(!ret)ret=read_reg(0x0751,&s.reg);
 if(!ret)ret=read_reg(0x1804,&s.pwm0);
 if(!ret)ret=read_reg(0x1809,&s.pwm1);
 if(ret)return ret;
 s.at=jiffies;s.valid=true;
 mutex_lock(&snap_lock);
 if(gen==snap_gen)snap=s;
 mutex_unlock(&snap_lock);
 *out=s;
 return 0;
}
/* EC fields no older than cache_ms. A stale cache is refreshed without the control
 * mutex, so readers never stall the lease worker or a control write. */
static int snapshot_get(struct snapshot *out){
 int ret=0;
 mutex_lock(&refresh_lock);
 mutex_lock(&snap_lock);*out=snap;mutex_unlock(&snap_lock);
 if(!out->valid||!time_before(jiffies,out->at+msecs_to_jiffies(cache_ms)))ret=refresh(out);
 mutex_unlock(&refresh_lock);
 return ret;
}
#define V1_FORMAT "mode=%d cpu_target=%u gpu_target=%u cpu_rpm=%d gpu_rpm=%d cpu_temp=%u gpu_temp=%u mode_byte=%u cpu_pwm=%u gpu_pwm=%u lease_ms=%d error=%d thermal=%d"
/* status: the v1 line, byte for byte. status2: the same fields plus the 0.3.0 ones. */
static int print_status(char *buffer,bool extended){
 struct snapshot s;int ret,m,e,remaining=0;unsigned int t0,t1,f0,f1,low,st;bool th,ld;
 ret=snapshot_get(&s);
 if(ret)return ret;
 mutex_lock(&lock);
 if(mode&&time_before(jiffies,deadline))remaining=jiffies_to_msecs(deadline-jiffies);
 m=mode;t0=targets[0];t1=targets[1];e=last_error;th=thermal_override;ld=allow_low_duty;low=min_duty;st=stalls;
 f0=floor_value(0);f1=floor_value(1);
 mutex_unlock(&lock);
 if(!extended)
  return scnprintf(buffer,PAGE_SIZE,V1_FORMAT "\n",m,t0,t1,s.r0,s.r1,s.cpu,s.gpu,s.reg,s.pwm0,s.pwm1,remaining,e,th);
 return scnprintf(buffer,PAGE_SIZE,V1_FORMAT " sample_age_ms=%u cache_ms=%u allow_low_duty=%d min_duty=%u cpu_floor=%u gpu_floor=%u stalls=%u\n",
  m,t0,t1,s.r0,s.r1,s.cpu,s.gpu,s.reg,s.pwm0,s.pwm1,remaining,e,th,jiffies_to_msecs(jiffies-s.at),cache_ms,ld,low,f0,f1,st);
}
static int status_get(char *buffer,const struct kernel_param *kp){return print_status(buffer,false);}
static int status2_get(char *buffer,const struct kernel_param *kp){return print_status(buffer,true);}
static int min_duty_set(const char *value,const struct kernel_param *kp){
 unsigned int d;int ret=kstrtouint(value,10,&d);
 if(ret)return ret;
 if(d<25||d>50)return -EINVAL; /* safe floor = spin-up + 5 %, at least 25 % (plan Faz 5) */
 mutex_lock(&lock);min_duty=d;mutex_unlock(&lock);
 return 0;
}
static int cache_ms_set(const char *value,const struct kernel_param *kp){
 unsigned int ms;int ret=kstrtouint(value,10,&ms);
 if(ret)return ret;
 if(ms<200||ms>5000)return -EINVAL;
 cache_ms=ms;invalidate();
 return 0;
}
static int allow_low_duty_set(const char *value,const struct kernel_param *kp){
 bool on;int ret=kstrtobool(value,&on);
 if(ret)return ret;
 mutex_lock(&lock);allow_low_duty=on;mutex_unlock(&lock);
 return 0;
}
static int allow_low_duty_get(char *buffer,const struct kernel_param *kp){return scnprintf(buffer,PAGE_SIZE,"%c\n",allow_low_duty?'Y':'N');}
static const struct kernel_param_ops control_ops={.set=control_set};
static const struct kernel_param_ops status_ops={.get=status_get};
static const struct kernel_param_ops status2_ops={.get=status2_get};
static const struct kernel_param_ops min_duty_ops={.set=min_duty_set,.get=param_get_uint};
static const struct kernel_param_ops cache_ms_ops={.set=cache_ms_set,.get=param_get_uint};
static const struct kernel_param_ops allow_low_duty_ops={.set=allow_low_duty_set,.get=allow_low_duty_get};
module_param_cb(control,&control_ops,NULL,0200);
module_param_cb(status,&status_ops,NULL,0400);
module_param_cb(status2,&status2_ops,NULL,0400);
module_param_cb(min_duty,&min_duty_ops,&min_duty,0644);
module_param_cb(cache_ms,&cache_ms_ops,&cache_ms,0644);
module_param_cb(allow_low_duty,&allow_low_duty_ops,NULL,0644);
MODULE_PARM_DESC(control,"auto | boost | manual <cpu%> <gpu%> (15 s lease)");
MODULE_PARM_DESC(status,"v1 status line (cached EC readings)");
MODULE_PARM_DESC(status2,"v1 fields plus sample_age_ms, cache and floor state");
MODULE_PARM_DESC(min_duty,"lowest manual duty, 25-50 % (default 50 until the low-duty sweep)");
MODULE_PARM_DESC(cache_ms,"maximum age of cached EC readings, 200-5000 ms (default 1000)");
MODULE_PARM_DESC(allow_low_duty,"test only: manual down to 20 % with stall detection (default N)");
/* hwmon: fan RPM, EC temperature and duty (0–255), read-only, from the cache. */
static int hw_value(const struct snapshot *s,enum hwmon_sensor_types type,u32 attr,int ch,long *val){
 if(type==hwmon_fan&&attr==hwmon_fan_input){*val=ch?s->r1:s->r0;return 0;}
 if(type==hwmon_temp&&attr==hwmon_temp_input){*val=(ch?s->gpu:s->cpu)*1000L;return 0;}
 if(type==hwmon_pwm&&attr==hwmon_pwm_input){*val=(ch?s->pwm1:s->pwm0)*255L/200;return 0;}
 return -EOPNOTSUPP;
}
static int hw_read(struct device *dev,enum hwmon_sensor_types type,u32 attr,int ch,long *val){
 struct snapshot s;int ret=snapshot_get(&s);
 return ret?ret:hw_value(&s,type,attr,ch,val);
}
static int hw_label(struct device *dev,enum hwmon_sensor_types type,u32 attr,int ch,const char **str){
 *str=ch?"GPU":"CPU";
 return 0;
}
static const struct hwmon_ops hw_ops={.visible=0444,.read=hw_read,.read_string=hw_label};
static const struct hwmon_channel_info *const hw_info[]={
 HWMON_CHANNEL_INFO(fan,HWMON_F_INPUT|HWMON_F_LABEL,HWMON_F_INPUT|HWMON_F_LABEL),
 HWMON_CHANNEL_INFO(temp,HWMON_T_INPUT|HWMON_T_LABEL,HWMON_T_INPUT|HWMON_T_LABEL),
 HWMON_CHANNEL_INFO(pwm,HWMON_PWM_INPUT,HWMON_PWM_INPUT),
 NULL
};
static const struct hwmon_chip_info hw_chip={.ops=&hw_ops,.info=hw_info};
static int sleep_event(struct notifier_block *nb,unsigned long event,void *data){
 int ret=0;
 if(event==PM_SUSPEND_PREPARE||event==PM_HIBERNATION_PREPARE||event==PM_RESTORE_PREPARE){
  /* Block controls and requeues before waiting for a running worker. */
  mutex_lock(&lock);suspended=true;mutex_unlock(&lock);
  cancel_delayed_work_sync(&work);
  mutex_lock(&lock);
  if(mode||auto_pending)ret=automatic();
  if(ret){suspended=false;queue_retry();}
  mutex_unlock(&lock);
  if(ret){pr_err("r9t_fan: automatic fan restore failed before sleep: %d\n",ret);return NOTIFY_BAD;}
 }else if(event==PM_POST_SUSPEND||event==PM_POST_HIBERNATION||event==PM_POST_RESTORE){
  mutex_lock(&lock);suspended=false;queue_retry();mutex_unlock(&lock);
  invalidate();
 }
 return NOTIFY_OK;
}
static struct notifier_block sleep_notifier={.notifier_call=sleep_event};
/* hwmon is a convenience: if it cannot register, the control path still loads. */
static void hw_register(void){
 pdev=platform_device_register_simple("r9t_fan",PLATFORM_DEVID_NONE,NULL,0);
 if(IS_ERR(pdev)){pr_warn("r9t_fan: no platform device, hwmon off: %ld\n",PTR_ERR(pdev));pdev=NULL;return;}
 hwmon=hwmon_device_register_with_info(&pdev->dev,"r9t_fan",NULL,&hw_chip,NULL);
 if(IS_ERR(hwmon)){pr_warn("r9t_fan: hwmon off: %ld\n",PTR_ERR(hwmon));hwmon=NULL;}
}
static void hw_unregister(void){
 if(hwmon)hwmon_device_unregister(hwmon);
 if(pdev)platform_device_unregister(pdev);
 hwmon=NULL;pdev=NULL;
}
static int __init fan_init(void){
 u8 project;int ret;
 if(!dmi_match(DMI_SYS_VENDOR,"GAME GARAJ")||!dmi_match(DMI_PRODUCT_NAME,"SLAYER R9T"))return -ENODEV;
 ret=read_reg(0x0740,&project);
 if(ret||project!=0x1a)return -ENODEV;
 ret=register_pm_notifier(&sleep_notifier);
 if(ret)return ret;
 hw_register();
 return 0;
}
static void __exit fan_exit(void){
 int ret=0,i;
 hw_unregister();
 unregister_pm_notifier(&sleep_notifier);
 mutex_lock(&lock);stopping=true;mutex_unlock(&lock);
 cancel_delayed_work_sync(&work);
 mutex_lock(&lock);
 for(i=0;(mode||auto_pending)&&i<3;i++){
  ret=automatic();if(!ret)break;
  if(i<2)msleep(100);
 }
 mutex_unlock(&lock);
 if(ret)pr_err("r9t_fan: automatic fan restore failed on unload: %d; firmware control is unconfirmed\n",ret);
}
module_init(fan_init);module_exit(fan_exit);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Slayer R9T fan control with RPM feedback and 15-second lease");
MODULE_VERSION("0.3.0");
MODULE_SOFTDEP("pre: uniwill_wmi tuxedo_keyboard");
