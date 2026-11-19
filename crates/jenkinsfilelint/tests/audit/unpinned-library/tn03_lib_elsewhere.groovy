// lib() outside a libraries block is not the directive.
def lib(String name) {
    return name
}

node('linux') {
    echo lib('shared@main')
    echo 'library shared@main'
}
