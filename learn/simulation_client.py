""" A simple class for communicating with a simulation gRPC service. """
import grpc
import tensorflow as tf

from simulate.proto import simulation_service_pb2
from simulate.proto import simulation_service_pb2_grpc

HOST = "localhost"
PORT = "50051"

def issue_query(stub, inp):
    """ Send out an async request to the ConductSimulation stub. """
    request = simulation_service_pb2.SimulationRequest(request_string=inp)
    response_future = stub.ConductSimulation.future(request)
    return response_future.result().response_string

def main():
    """ Entry point into simulation client. """
    channel = grpc.insecure_channel(HOST+":"+PORT)
    stub = simulation_service_pb2_grpc.SimulationServiceStub(channel)
    value = issue_query(stub, "MARIO")

    print(value)
    return 0

if __name__ == '__main__':
    main()
