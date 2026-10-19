/* Fault and lifecycle scenarios for the real r9t_fan.c (included before this file).
 * No module is loaded and no hardware is accessed. */
#define CHECK(x) do{if(!(x)){fprintf(stderr,"line %d failed: %s\n",__LINE__,#x);return 1;}}while(0)
static void reset(void){mode=0;auto_pending=false;suspended=false;stopping=false;thermal_override=false;last_error=0;jiffies=0;deadline=15000;queued=0;pwm_writes=0;reads=0;writes=0;fail_read=0;fail_write=0;fail_auto=0;stuck_bit=0;logs=0;memset(ec,0,sizeof(ec));ec[0x0751]=0x40;ec[0x043e]=50;ec[0x044f]=50;}
static void tick(void){queued=0;worker(NULL);}
int main(void){
 char status[PAGE_SIZE];
 /* Failed lease restoration retains retry ownership; successful retry clears it. */
 reset();mode=1;jiffies=15000;fail_read=1;tick();
 CHECK(mode==1&&auto_pending&&queued&&last_error==-EAGAIN&&pwm_writes==0);
 jiffies=16000;tick();CHECK(queued&&pwm_writes==0);
 fail_read=0;tick();CHECK(mode==0&&!auto_pending&&!queued&&!(ec[0x0751]&0x40));
 /* Explicit auto failure from an idle state also owns a retry. */
 reset();fail_write=1;CHECK(control_set("auto",NULL)==-EIO);
 CHECK(auto_pending&&queued&&last_error==-EIO);
 CHECK(control_set("manual 70 70",NULL)==-EBUSY);
 fail_write=0;tick();CHECK(!auto_pending&&!queued);
 /* Upstream error and silent EC refusal both remain visible. */
 reset();mode=2;fail_auto=-EIO;CHECK(automatic()==-EIO);CHECK(auto_pending&&mode==2);
 fail_auto=0;stuck_bit=1;CHECK(automatic()==-EIO);CHECK(auto_pending);
 stuck_bit=0;CHECK(automatic()==0);CHECK(!auto_pending&&mode==0);
 /* An update fault immediately switches to restoration, never future manual writes. */
 reset();mode=1;fail_read=1;tick();CHECK(auto_pending&&queued&&pwm_writes==0);
 fail_read=0;tick();CHECK(mode==0&&!queued);
 /* A new manual lease that fails its first PWM write must queue fallback too. */
 reset();fail_write=1;CHECK(control_set("manual 70 80",NULL)==-EIO);
 CHECK(auto_pending&&queued&&mode==1);
 fail_write=0;tick();CHECK(mode==0&&!auto_pending&&!queued);
 /* Preserve the existing strict userland status key contract. */
 reset();CHECK(status_get(status,NULL)>0);CHECK(!strstr(status,"auto_pending="));
 CHECK(strstr(status,"thermal=")&&strstr(status,"error="));
 /* Successful suspend quiesces worker and rejects controls until resume. */
 reset();mode=1;queued=1;CHECK(sleep_event(NULL,PM_SUSPEND_PREPARE,NULL)==NOTIFY_OK);
 CHECK(suspended&&!queued&&mode==0);CHECK(control_set("boost",NULL)==-EBUSY);
 CHECK(sleep_event(NULL,PM_POST_SUSPEND,NULL)==NOTIFY_OK);CHECK(!suspended);
 /* Failed suspend is vetoed while restoration retries continue. */
 reset();mode=1;fail_read=1;CHECK(sleep_event(NULL,PM_SUSPEND_PREPARE,NULL)==NOTIFY_BAD);
 CHECK(!suspended&&auto_pending&&queued&&logs==1);
 fail_read=0;tick();CHECK(!auto_pending&&!queued);
 /* Exit tries finitely, reports failure, and leaves no work queued. */
 reset();mode=1;queued=1;fail_read=1;fan_exit();
 CHECK(stopping&&!queued&&reads==3&&logs==1);
 tick();CHECK(!queued&&reads==3);
 CHECK(control_set("auto",NULL)==-EBUSY);
 reset();mode=1;queued=1;fan_exit();CHECK(stopping&&!queued&&mode==0&&logs==0);
 puts("fan fallback fault harness: all scenarios passed");return 0;
}
