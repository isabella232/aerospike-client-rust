// Copyright 2015-2017 Aerospike, Inc.
//
// Portions may be licensed to Aerospike, Inc. under one or more contributor
// license agreements.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not
// use this file except in compliance with the License. You may obtain a copy of
// the License at http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations under
// the License.

use std::sync::Arc;
use std::vec::Vec;
use std::thread;
use std::str;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use rustc_serialize::base64::{ToBase64, FromBase64, STANDARD};
use threadpool::ThreadPool;

use errors::*;
use Bin;
use CollectionIndexType;
use IndexType;
use Key;
use Record;
use Recordset;
use ResultCode;
use Statement;
use UDFLang;
use Value;
use cluster::{Cluster, Node};
use commands::{ReadCommand, WriteCommand, DeleteCommand, TouchCommand, ExistsCommand,
               ReadHeaderCommand, OperateCommand, ExecuteUDFCommand, ScanCommand, QueryCommand};
use net::ToHosts;
use operations::{Operation, OperationType};
use policy::{ClientPolicy, ReadPolicy, WritePolicy, ScanPolicy, QueryPolicy};


// Client encapsulates an Aerospike cluster.
// All database operations are available against this object.
pub struct Client {
    pub cluster: Arc<Cluster>,
    thread_pool: ThreadPool,
}

unsafe impl Send for Client {}
unsafe impl Sync for Client {}

impl Client {
    pub fn new(policy: &ClientPolicy, hosts: &ToHosts) -> Result<Self> {
        let hosts = try!(hosts.to_hosts());
        let cluster = try!(Cluster::new(policy.clone(), &hosts));
        let thread_pool = ThreadPool::new(policy.thread_pool_size);

        Ok(Client {
            cluster: cluster,
            thread_pool: thread_pool,
        })
    }

    pub fn close(&self) -> Result<()> {
        self.cluster.close()
    }

    pub fn is_connected(&self) -> bool {
        self.cluster.is_connected()
    }

    pub fn nodes(&self) -> Result<Vec<Arc<Node>>> {
        Ok(self.cluster.nodes())
    }

    pub fn get(&self,
                       policy: &ReadPolicy,
                       key: &Key,
                       bin_names: Option<&[&str]>)
                       -> Result<Record> {
        let mut command = ReadCommand::new(policy, self.cluster.clone(), key, bin_names);
        try!(command.execute());
        Ok(command.record.unwrap())
    }

    pub fn get_header(&self,
                              policy: &ReadPolicy,
                              key: &Key)
                              -> Result<Record> {
        let mut command = ReadHeaderCommand::new(policy, self.cluster.clone(), key);
        try!(command.execute());
        Ok(command.record.unwrap())
    }

    pub fn put(&self,
                       policy: &WritePolicy,
                       key: &Key,
                       bins: &[&Bin])
                       -> Result<()> {
        let mut command =
            WriteCommand::new(policy, self.cluster.clone(), key, bins, OperationType::Write);
        command.execute()
    }

    pub fn add(&self,
                       policy: &WritePolicy,
                       key: &Key,
                       bins: &[&Bin])
                       -> Result<()> {
        let mut command =
            WriteCommand::new(policy, self.cluster.clone(), key, bins, OperationType::Incr);
        command.execute()
    }

    pub fn append(&self,
                          policy: &WritePolicy,
                          key: &Key,
                          bins: &[&Bin])
                          -> Result<()> {
        let mut command =
            WriteCommand::new(policy, self.cluster.clone(), key, bins, OperationType::Append);
        command.execute()
    }

    pub fn prepend(&self,
                           policy: &WritePolicy,
                           key: &Key,
                           bins: &[&Bin])
                           -> Result<()> {
        let mut command =
            WriteCommand::new(policy, self.cluster.clone(), key, bins, OperationType::Prepend);
        command.execute()
    }

    pub fn delete(&self,
                          policy: &WritePolicy,
                          key: &Key)
                          -> Result<bool> {
        let mut command = DeleteCommand::new(policy, self.cluster.clone(), key);
        try!(command.execute());
        Ok(command.existed)
    }

    pub fn touch(&self, policy: &WritePolicy, key: &Key) -> Result<()> {
        let mut command = TouchCommand::new(policy, self.cluster.clone(), key);
        command.execute()
    }

    pub fn exists(&self,
                          policy: &WritePolicy,
                          key: &Key)
                          -> Result<bool> {
        let mut command = ExistsCommand::new(policy, self.cluster.clone(), key);
        try!(command.execute());
        Ok(command.exists)
    }

    pub fn operate(&self,
                           policy: &WritePolicy,
                           key: &Key,
                           ops: &[Operation])
                           -> Result<Record> {
        let mut command = OperateCommand::new(policy, self.cluster.clone(), key, ops);
        try!(command.execute());
        Ok(command.read_command.record.unwrap())
    }

    /// //////////////////////////////////////////////////////////////////////////

    pub fn register_udf(&self,
                                policy: &WritePolicy,
                                udf_body: &[u8],
                                udf_name: &str,
                                language: UDFLang)
                                -> Result<()> {
        let udf_body = udf_body.to_base64(STANDARD);

        let cmd = format!("udf-put:filename={};content={};content-len={};udf-type={};",
                          udf_name,
                          udf_body,
                          udf_body.len(),
                          language);
        let node = try!(self.cluster.get_random_node());
        let response = try!(node.info(policy.base_policy.timeout, &[&cmd]));

        if let Some(msg) = response.get("error") {
            let msg = try!(msg.from_base64());
            let msg = try!(str::from_utf8(&msg));
            bail!("UDF Registration failed: {}, file: {}, line: {}, message: {}",
                  response.get("error").unwrap_or(&"-".to_string()),
                  response.get("file").unwrap_or(&"-".to_string()),
                  response.get("line").unwrap_or(&"-".to_string()),
                  msg);
        }

        Ok(())
    }

    pub fn register_udf_from_file(&self,
                                          policy: &WritePolicy,
                                          client_path: &str,
                                          udf_name: &str,
                                          language: UDFLang)
                                          -> Result<()> {

        let path = Path::new(client_path);
        let mut file = try!(File::open(&path));
        let mut udf_body: Vec<u8> = vec![];
        try!(file.read_to_end(&mut udf_body));

        self.register_udf(policy, &udf_body, udf_name, language)
    }

    pub fn remove_udf(&self,
                              policy: &WritePolicy,
                              udf_name: &str,
                              language: UDFLang)
                              -> Result<()> {

        let cmd = format!("udf-remove:filename={}.{};", udf_name, language);
        let node = try!(self.cluster.get_random_node());
        let response = try!(node.info(policy.base_policy.timeout, &[&cmd]));

        if let Some(_) = response.get("ok") {
            return Ok(());
        }

        bail!("UDF Remove failed: {:?}", response)
    }

    pub fn execute_udf(&self,
                               policy: &WritePolicy,
                               key: &Key,
                               udf_name: &str,
                               function_name: &str,
                               args: Option<&[Value]>)
                               -> Result<Option<Value>> {

        let mut command = ExecuteUDFCommand::new(policy,
                                                      self.cluster.clone(),
                                                      key,
                                                      udf_name,
                                                      function_name,
                                                      args);
        try!(command.execute());

        let record = command.read_command.record.as_ref().unwrap().clone();

        // User defined functions don't have to return a value.
        if record.bins.len() == 0 {
            return Ok(None);
        }

        for (key, value) in record.bins.iter() {
            if key.contains("SUCCESS") {
                return Ok(Some(value.clone()));
            } else if key.contains("FAILURE") {
                bail!("{:?}", value);
            }
        }

        Err("Invalid UDF return value".into())
    }

    pub fn scan(&self,
                        policy: &ScanPolicy,
                        namespace: &str,
                        set_name: &str,
                        bin_names: Option<&[&str]>)
                        -> Result<Arc<Recordset>> {

        let bin_names = match bin_names {
            None => None,
            Some(bin_names) => {
                let bin_names: Vec<_> = bin_names.iter().cloned().map(String::from).collect();
                Some(bin_names)
            }
        };

        let nodes = self.cluster.nodes();
        let recordset = Arc::new(Recordset::new(policy.record_queue_size, nodes.len()));
        for node in nodes {
            let node = node.clone();
            let recordset = recordset.clone();
            let policy = policy.to_owned();
            let namespace = namespace.to_owned();
            let set_name = set_name.to_owned();
            let bin_names = bin_names.to_owned();

            thread::spawn(move || {
                let mut command = ScanCommand::new(&policy, node, &namespace, &set_name, &bin_names, recordset);
                command.execute().unwrap();
            });

        }
        Ok(recordset)
    }

    pub fn scan_node(&self,
                             policy: &ScanPolicy,
                             node: Node,
                             namespace: &str,
                             set_name: &str,
                             bin_names: Option<&[&str]>)
                             -> Result<Arc<Recordset>> {


        let bin_names = match bin_names {
            None => None,
            Some(bin_names) => {
                let bin_names: Vec<_> = bin_names.iter().cloned().map(String::from).collect();
                Some(bin_names)
            }
        };

        let recordset = Arc::new(Recordset::new(policy.record_queue_size, 1));
        let node = Arc::new(node).clone();
        let t_recordset = recordset.clone();
        let policy = policy.to_owned();
        let namespace = namespace.to_owned();
        let set_name = set_name.to_owned();
        let bin_names = bin_names.to_owned();

        self.thread_pool.execute(move || {
            let mut command = ScanCommand::new(&policy,
                                               node,
                                               &namespace,
                                               &set_name,
                                               &bin_names,
                                               t_recordset);
            command.execute().unwrap();
        });

        Ok(recordset)
    }

    pub fn query(&self,
                         policy: &QueryPolicy,
                         statement: Statement)
                         -> Result<Arc<Recordset>> {

        try!(statement.validate());
        let statement = Arc::new(statement);

        let nodes = self.cluster.nodes();
        let recordset = Arc::new(Recordset::new(policy.record_queue_size, nodes.len()));
        for node in nodes {
            let node = node.clone();
            let t_recordset = recordset.clone();
            let policy = policy.to_owned();
            let statement = statement.clone();

            self.thread_pool.execute(move || {
                let mut command = QueryCommand::new(&policy, node, statement, t_recordset);
                command.execute().unwrap();
            });
        }
        Ok(recordset)
    }

    pub fn query_node(&self,
                              policy: &QueryPolicy,
                              node: Node,
                              statement: Statement)
                              -> Result<Arc<Recordset>> {

        try!(statement.validate());

        let recordset = Arc::new(Recordset::new(policy.record_queue_size, 1));
        let node = Arc::new(node).clone();
        let t_recordset = recordset.clone();
        let policy = policy.to_owned();
        let statement = Arc::new(statement).clone();

        self.thread_pool.execute(move || {
            let mut command = QueryCommand::new(&policy, node, statement, t_recordset);
            command.execute().unwrap();
        });

        Ok(recordset)
    }

    pub fn create_index(&self,
                        policy: &WritePolicy,
                        namespace: &str,
                        set_name: &str,
                        bin_name: &str,
                        index_name: &str,
                        index_type: IndexType)
                        -> Result<()> {
        self.create_complex_index(policy,
                                  namespace,
                                  set_name,
                                  bin_name,
                                  index_name,
                                  index_type,
                                  CollectionIndexType::Default)

    }

    pub fn create_complex_index(&self,
                                policy: &WritePolicy,
                                namespace: &str,
                                set_name: &str,
                                bin_name: &str,
                                index_name: &str,
                                index_type: IndexType,
                                collection_index_type: CollectionIndexType)
                                -> Result<()> {
        let cit_str: String = match collection_index_type {
            CollectionIndexType::Default => "".to_string(),
            _ => format!("indextype={};", collection_index_type),
        };
        let cmd = format!("sindex-create:ns={};set={};indexname={};numbins=1;{}indexdata={},{};priority=normal",
                          namespace, set_name, index_name, cit_str, bin_name, index_type);
        self.send_sindex_cmd(cmd, &policy).chain_err(|| "Error creating index")
    }


    pub fn drop_index(&self,
                      policy: &WritePolicy,
                      namespace: &str,
                      set_name: &str,
                      index_name: &str)
                      -> Result<()> {

        let set_name: String = match set_name {
            "" => "".to_string(),
            _ => format!("set={};", set_name),
        };
        let cmd = format!("sindex-delete:ns={};{}indexname={}",
                          namespace, set_name, index_name);
        self.send_sindex_cmd(cmd, &policy).chain_err(|| "Error dropping index")
    }

    fn send_sindex_cmd(&self, cmd: String, policy: &WritePolicy) -> Result<()> {
        let node = try!(self.cluster.get_random_node());
        let response = try!(node.info(policy.base_policy.timeout, &[&cmd]));

        for v in response.values() {
            if v.to_uppercase() == "OK" {
                return Ok(())
            } else if v.starts_with("FAIL:200") {
                bail!(ErrorKind::ServerError(ResultCode::from(200)));
            } else if v.starts_with("FAIL:201") {
                bail!(ErrorKind::ServerError(ResultCode::from(201)));
            } else {
                break;
            }
        }

        bail!(ErrorKind::BadResponse("Unexpected sindex info command response".to_string()))
    }
}