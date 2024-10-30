use jni::JNIEnv;
use payload::Payload;
use jni::objects::{JClass, JString};

use jni::sys::jstring;

mod payload;
mod chromeos_update_engine;

#[no_mangle]
pub extern "system" fn Java_com_rajmani7584_payloaddumper_MainActivity_getPartitionList<'local>(mut env: JNIEnv<'local>, _class: JClass<'local>, path: JString<'local>) -> jstring {

    let mut msg: String = Default::default();

    let mut payload = match Payload::new(env.get_string(&path).expect("Err: msg").into()) {
        Ok(p) => {
            p
        }
        Err(err) => {
            return env.new_string(format!("Err:{}", err)).expect("Err:expect").into_raw();
        }
    };
    
    let _ = match payload.get_partition_list() {
        Ok(res) => {
            msg.insert_str(msg.len(), &res);
        }
        Err(err) => {
            return env.new_string(format!("Err:{}", err)).expect("Err:expect").into_raw();
        }
    };

    let msg = env.new_string(msg).expect("Err:expect").into_raw();

    return msg;
}


#[no_mangle]
pub extern "system" fn Java_com_rajmani7584_payloaddumper_MainActivity_extractPartition<'local>(mut env: JNIEnv<'local>, _class: JClass<'local>, path: JString<'local>, partition: JString<'local>, out_path: JString<'local>) -> jstring {
    
    let mut msg: String = Default::default();

    let path: String = env.get_string(&path).expect("Err:expect").into();

    let mut payload = match Payload::new(path) {
        Ok(p) => {
            p
        }
        Err(err) => {
            return env.new_string(format!("Err:{}", err)).expect("Err:expect").into_raw();
        }
    };

    let partition: String = env.get_string(&partition).expect("Err:expect").into();
    let out: String = env.get_string(&out_path).expect("Err:expect").into();

    let _ = match payload.extract(&partition, &out) {
        Ok(_res) => {
            msg.insert_str(msg.len(), "Done");
        }
        Err(err) => {
            return env.new_string(format!("Err:{}", err)).expect("Err:expect").into_raw();
        }
    };

    return env.new_string(msg).expect("Err:expect").into_raw();
}