from flask import Blueprint

bp = Blueprint("errors", __name__)
from portunus.errors import handlers
