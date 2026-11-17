def load(String path) throws IOException, IllegalStateException {
    return readFile(path)
}

def 'quoted name'() {
    return 1
}

@NonCPS
static String join(List<String> xs) {
    return xs.join(',')
}

pipeline {
    agent any
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
