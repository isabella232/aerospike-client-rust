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

use Bin;
use operations::*;

impl<'a> Operation<'a> {

    pub fn get() -> Self {
        Operation {
            op: OperationType::Read,
            bin: OperationBin::All,
            data: OperationData::None,
        }
    }

    pub fn get_header() -> Self {
        Operation {
            op: OperationType::Read,
            bin: OperationBin::None,
            data: OperationData::None,
        }
    }

    pub fn get_bin(bin_name: &'a str) -> Self {
        Operation {
            op: OperationType::Read,
            bin: OperationBin::Name(bin_name),
            data: OperationData::None,
        }
    }

    pub fn put(bin: &'a Bin) -> Self {
        Operation {
            op: OperationType::Write,
            bin: OperationBin::Name(bin.name),
            data: OperationData::Value(&bin.value),
        }
    }

    pub fn append(bin: &'a Bin) -> Self {
        Operation {
            op: OperationType::Append,
            bin: OperationBin::Name(bin.name),
            data: OperationData::Value(&bin.value),
        }
    }

    pub fn prepend(bin: &'a Bin) -> Self {
        Operation {
            op: OperationType::Prepend,
            bin: OperationBin::Name(bin.name),
            data: OperationData::Value(&&bin.value),
        }
    }

    pub fn incr(bin: &'a Bin) -> Self {
        Operation {
            op: OperationType::Incr,
            bin: OperationBin::Name(bin.name),
            data: OperationData::Value(&bin.value),
        }
    }

    pub fn touch() -> Self {
        Operation {
            op: OperationType::Touch,
            bin: OperationBin::None,
            data: OperationData::None,
        }
    }

}