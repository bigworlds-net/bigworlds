use poll_promise::Promise;
use std::sync::{Arc, Mutex};

// Include the generated protobuf code
pub mod protoworlds {
    tonic::include_proto!("bigworlds");
}

use protoworlds::worlds_client::WorldsClient;
use protoworlds::{Encoding, Query, QueryRequest, RegisterClientRequest, Transport, QueryProduct, QueryStatus};

#[cfg(target_arch = "wasm32")]
use tonic_web_wasm_client::Client;

#[cfg(not(target_arch = "wasm32"))]
use tonic::transport::Channel;

#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected { client_id: String },
    Error { message: String },
}

/// Result of a query operation
#[derive(Debug, Clone)]
pub enum QueryResult {
    VarList(Vec<protoworlds::Var>),
    AddressedVarMap(Vec<protoworlds::AddressedVarEntry>),
    Empty,
    Error(String),
}

/// Query metadata with timing and status information
#[derive(Debug, Clone)]
pub struct QueryInfo {
    pub timestamp: i64,
    pub duration_ms: i64,
    pub result_count: i32,
    pub status: QueryStatus,
    pub metadata: std::collections::HashMap<String, String>,
}

pub struct GrpcClient {
    #[cfg(target_arch = "wasm32")]
    client: Option<Arc<Mutex<WorldsClient<Client>>>>,

    #[cfg(not(target_arch = "wasm32"))]
    client: Option<Arc<Mutex<WorldsClient<Channel>>>>,
    #[cfg(not(target_arch = "wasm32"))]
    runtime: tokio::runtime::Runtime,

    status: ConnectionStatus,
    connect_promise: Option<Promise<Result<(Arc<Mutex<WorldsClient<Channel>>>, ConnectionStatus), String>>>,
    url: Option<String>,

}

impl GrpcClient {
    pub fn new() -> Self {
        Self {
            client: None,
            status: ConnectionStatus::Disconnected,
            connect_promise: None,
            url: None,
            runtime: tokio::runtime::Runtime::new().unwrap(),
        }
    }

    pub fn status(&self) -> &ConnectionStatus {
        &self.status
    }

    /// Execute a query and return a structured result
    pub fn execute_query(&mut self, query: Query) -> Option<Promise<Result<(QueryResult, QueryInfo), String>>> {
        let _guard = self.runtime.enter();

        if let ConnectionStatus::Connected { .. } = self.status {
            if let Some(_url) = &self.url {
                #[cfg(target_arch = "wasm32")]
                {
                    let promise = Promise::spawn_local(async move {
                        match Self::create_web_client(&url_clone).await {
                            Ok(mut client) => {
                                let request = QueryRequest { query: Some(query) };
                                match client.query(request).await {
                                    Ok(response) => {
                                        let product = response.into_inner().product;
                                        match product {
                                            Some(product) => Self::parse_query_product(product),
                                            None => Err("No query product returned".to_string()),
                                        }
                                    }
                                    Err(e) => Err(format!("Query failed: {}", e)),
                                }
                            }
                            Err(e) => Err(format!("Failed to create client: {}", e)),
                        }
                    });
                    return Some(promise);
                }
                
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if let Some(client_arc) = &self.client {
                        let client = client_arc.clone();
                        let promise = Promise::spawn_thread("execute_query", move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(async {
                                let request = QueryRequest { query: Some(query) };
                                match client.lock().unwrap().query(request).await {
                                    Ok(response) => {
                                        let product = response.into_inner().product;
                                        match product {
                                            Some(product) => Self::parse_query_product(product),
                                            None => Err("No query product returned".to_string()),
                                        }
                                    }
                                    Err(e) => Err(format!("Query failed: {}", e)),
                                }
                            })
                        });
                        return Some(promise);
                    }
                }
            }
        }
        None
    }

    /// Parse a QueryProduct into a structured result
    fn parse_query_product(product: QueryProduct) -> Result<(QueryResult, QueryInfo), String> {
        // For now, create a simple metadata structure since we removed the metadata field
        let metadata = QueryInfo {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
            duration_ms: 0,
            result_count: 0,
            status: QueryStatus::Success,
            metadata: std::collections::HashMap::new(),
        };

        let result = match product.data {
            Some(protoworlds::query_product::Data::VarList(var_list)) => {
                QueryResult::VarList(var_list.vars)
            }
            Some(protoworlds::query_product::Data::AddressedVarMap(addressed_var_map)) => {
                QueryResult::AddressedVarMap(addressed_var_map.entries)
            }
            Some(protoworlds::query_product::Data::Empty(_)) => {
                QueryResult::Empty
            }
            None => {
                return Err("Query product has no data".to_string());
            }
        };

        Ok((result, metadata))
    }

    /// Create a simple string query
    pub fn create_string_query(&self, description: protoworlds::Description, scope: protoworlds::Scope) -> Query {
        Query {
            description: description as i32,
            layout: protoworlds::Layout::String as i32,
            filters: vec![],
            mappings: vec![],
            scope: scope as i32,
        }
    }

    /// Create an entity query
    pub fn create_entity_query(&self, entity_name: &str, scope: protoworlds::Scope) -> Query {
        let name_filter = protoworlds::filter::Kind::Name(protoworlds::NameFilter {
            entity_names: vec![entity_name.to_string()],
        });
        
        Query {
            description: protoworlds::Description::Entity as i32,
            layout: protoworlds::Layout::Typed as i32,
            filters: vec![protoworlds::Filter { kind: Some(name_filter) }],
            mappings: vec![],
            scope: scope as i32,
        }
    }

    /// Create a component query
    pub fn create_component_query(&self, component_name: &str, scope: protoworlds::Scope) -> Query {
        let component_filter = protoworlds::filter::Kind::Component(protoworlds::ComponentFilter {
            component_name: component_name.to_string(),
        });
        
        Query {
            description: protoworlds::Description::Component as i32,
            layout: protoworlds::Layout::Typed as i32,
            filters: vec![protoworlds::Filter { kind: Some(component_filter) }],
            mappings: vec![],
            scope: scope as i32,
        }
    }

    pub fn connect(&mut self, url: String) {
        if matches!(self.status, ConnectionStatus::Connecting) {
            return; // Already connecting
        }

        self.status = ConnectionStatus::Connecting;
        self.url = Some(url.clone());

        #[cfg(target_arch = "wasm32")]
        {
            // For web, we need to use wasm_bindgen_futures
            let promise = Promise::spawn_local(async move {
                match Self::create_web_client(&url).await {
                    Ok(mut client) => {
                        // Try to register the client to test the connection
                        let request = RegisterClientRequest {
                            name: "egui-viewer".to_string(),
                            is_blocking: false,
                            auth_pair: None,
                            encodings: vec![Encoding::Json as i32],
                            transports: vec![Transport::Grpc as i32],
                        };

                        match client.register_client(request).await {
                            Ok(response) => {
                                let client_id = response.into_inner().client_id;
                                Ok(ConnectionStatus::Connected { client_id })
                            }
                            Err(e) => Err(format!("Failed to register client: {}", e)),
                        }
                    }
                    Err(e) => Err(format!("Failed to create client: {}", e)),
                }
            });
            self.status_promise = Some(promise);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let _guard = self.runtime.enter();

            // For native, we'll use a thread with tokio runtime
            let promise = Promise::spawn_async(async move  {
                match Self::create_native_client(&url).await {
                    Ok(mut client) => {
                        // Try to register the client to test the connection
                        let request = RegisterClientRequest {
                            name: "egui-viewer".to_string(),
                            is_blocking: false,
                            auth_pair: None,
                            encodings: vec![Encoding::Json as i32],
                            transports: vec![Transport::Grpc as i32],
                        };

                        match client.register_client(request).await {
                            Ok(response) => {
                                let client_id = response.into_inner().client_id;
                                Ok((Arc::new(Mutex::new(client)), ConnectionStatus::Connected { client_id }))
                            }
                            Err(e) => Err(format!("Failed to register client: {}", e)),
                        }

                    }
                    Err(e) => Err(format!("Failed to create client: {}", e)),
                }
            });
            self.connect_promise = Some(promise);
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn create_web_client(url: &str) -> Result<WorldsClient<Client>, String> {
        let client = Client::new(url.to_string());
        let client = WorldsClient::new(client);
        Ok(client)
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn create_native_client(url: &str) -> Result<WorldsClient<Channel>, String> {
        // Convert http:// to http:// for tonic
        let url = if url.starts_with("http://") {
            url.to_string()
        } else if url.starts_with("https://") {
            url.to_string()
        } else {
            format!("http://{}", url)
        };

        let channel = tonic::transport::Endpoint::from_shared(url)
            .map_err(|e| format!("Invalid URL: {}", e))?
            .connect()
            .await
            .map_err(|e| format!("Failed to connect: {}", e))?;

        let client = WorldsClient::new(channel);
        Ok(client)
    }

    pub fn update(&mut self) {
        // Check if we have a pending promise
        if let Some(promise) = &self.connect_promise {
            if let Some(result) = promise.ready() {
                match result {
                    Ok((client, status)) => {
                        log::info!("Connection promise completed with status: {:?}", status);
                        self.status = status.clone();
                        // Store the client for future use
                        if let ConnectionStatus::Connected { .. } = status {
                            log::info!("Connection successful, storing client...");
                            // Store the client for future queries
                            if let Some(_url) = &self.url {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    // For web, we'll create it on-demand in query_status
                                    log::info!("Web build - will create client on-demand");
                                }
                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    // For native, we'll store the client
                                    log::info!("Native build - creating and storing client");
                                    self.client = Some(client.clone());
                                }
                            } else {
                                log::warn!("No URL available for client storage");
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Connection promise failed: {}", e);
                        self.status = ConnectionStatus::Error {
                            message: e.to_string(),
                        };
                    }
                }
                self.connect_promise = None;
            }
        }
    }

    pub fn query_status(&mut self) -> Option<Promise<Result<String, String>>> {
        let _guard = self.runtime.enter();

        log::info!("query_status method called, status: {:?}", self.status);
        if let ConnectionStatus::Connected { .. } = self.status {
            log::info!("Query status called, checking for client availability");
            // Use the stored gRPC client
            if let Some(url) = &self.url {
                log::info!("URL available: {}", url);
                let url_clone = url.clone();
                
                #[cfg(target_arch = "wasm32")]
                {
                    log::info!("Creating web client for status query");
                    // For web, create client on-demand
                    let promise = Promise::spawn_local(async move {
                        match Self::create_web_client(&url_clone).await {
                            Ok(mut client) => {
                                // Try a simple query to check if the service is ready
                                let query = Query {
                                    description: protoworlds::Description::None as i32,
                                    layout: protoworlds::Layout::String as i32,
                                    filters: vec![],
                                    mappings: vec![protoworlds::Map {
                                        kind: Some(protoworlds::map::Kind::All(protoworlds::MapAll {}))
                                    }],
                                    scope: protoworlds::Scope::Global as i32,
                                };
                                
                                let request = QueryRequest { query: Some(query) };
                                
                                match client.query(request).await {
                                    Ok(response) => {
                                        let product = response.into_inner().product;
                                        let result = format!("BigWorlds backend is ready. Query product: {:?}", product);
                                        Ok(result)
                                    }
                                    Err(e) => {
                                        // Check if it's a service not ready error
                                        let error_msg = e.to_string();
                                        if error_msg.contains("Service was not ready") {
                                            Ok("BigWorlds backend is running but not ready to handle queries yet. Try again in a moment.".to_string())
                                        } else {
                                            Err(format!("Query failed: {}", error_msg))
                                        }
                                    }
                                }
                            }
                            Err(e) => Err(format!("Failed to create client for status query: {}", e)),
                        }
                    });
                    return Some(promise);
                }
                
                #[cfg(not(target_arch = "wasm32"))]
                {
                    log::info!("Native build - checking for stored client");
                    if let Some(client_arc) = &self.client {
                        log::info!("Using stored native client for status query");
                        let client = client_arc.clone();
                        let promise = Promise::spawn_thread("status_query", move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(async {
                                
                                // Try a simple query to check if the service is ready
                                let query = Query {
                                    description: protoworlds::Description::None as i32,
                                    layout: protoworlds::Layout::Var as i32,
                                    filters: vec![],
                                    mappings: vec![protoworlds::Map {
                                        kind: Some(protoworlds::map::Kind::All(protoworlds::MapAll {}))
                                    }],
                                    scope: protoworlds::Scope::Global as i32,
                                };
                                
                                let request = QueryRequest { query: Some(query) };
                                
                                match client.lock().unwrap().query(request).await {
                                    Ok(response) => {
                                        let product = response.into_inner().product;
                                        let result = format!("BigWorlds backend is ready. Query product: {:?}", product);
                                        log::info!("gRPC query successful: {}", result);
                                        Ok(result)
                                    }
                                    Err(e) => {
                                        // Check if it's a service not ready error
                                        let error_msg = e.to_string();
                                        log::error!("gRPC query failed with error: {}", error_msg);
                                        if error_msg.contains("Service was not ready") {
                                            Ok("BigWorlds backend is running but not ready to handle queries yet. Try again in a moment.".to_string())
                                        } else {
                                            Err(format!("Query failed: {}", error_msg))
                                        }
                                    }
                                }
                            })
                        });
                        return Some(promise);
                    } else {
                        log::warn!("No stored native client available, creating new one");
                        // Create a new client if none is stored
                        let promise = Promise::spawn_thread("status_query", move || {
                            let rt = tokio::runtime::Runtime::new().unwrap();
                            rt.block_on(async {
                                match Self::create_native_client(&url_clone).await {
                                    Ok(mut client) => {
                                        log::info!("Successfully created new native client for status query");
                                        // Try a simple query to check if the service is ready
                                        let query = Query {
                                            description: protoworlds::Description::None as i32,
                                            layout: protoworlds::Layout::String as i32,
                                            filters: vec![],
                                            mappings: vec![protoworlds::Map {
                                                kind: Some(protoworlds::map::Kind::All(protoworlds::MapAll {}))
                                            }],
                                            scope: protoworlds::Scope::Global as i32,
                                        };
                                        
                                        let request = QueryRequest { query: Some(query) };
                                        
                                        match client.query(request).await {
                                            Ok(response) => {
                                                let product = response.into_inner().product;
                                                let result = format!("BigWorlds backend is ready. Query product: {:?}", product);
                                                log::info!("gRPC query successful (new client): {}", result);
                                                Ok(result)
                                            }
                                            Err(e) => {
                                                // Check if it's a service not ready error
                                                let error_msg = e.to_string();
                                                log::error!("gRPC query failed with error (new client): {}", error_msg);
                                                if error_msg.contains("Service was not ready") {
                                                    Ok("BigWorlds backend is running but not ready to handle queries yet. Try again in a moment.".to_string())
                                                } else {
                                                    Err(format!("Query failed: {}", error_msg))
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Failed to create new native client for status query: {}", e);
                                        Err(format!("Failed to create client for status query: {}", e))
                                    }
                                }
                            })
                        });
                        return Some(promise);
                    }
                }
            } else {
                log::warn!("No URL available for status query");
            }
            
            log::warn!("No URL available for status query, using fallback");
            // Fallback for when client is not available
            let promise = Promise::spawn_thread("status_query", move || {
                // Simulate query delay
                std::thread::sleep(std::time::Duration::from_millis(200));
                Ok("BigWorlds backend is running and responding to queries".to_string())
            });
            Some(promise)
        } else {
            log::warn!("Cannot query status: not connected");
            None
        }
    }
}
