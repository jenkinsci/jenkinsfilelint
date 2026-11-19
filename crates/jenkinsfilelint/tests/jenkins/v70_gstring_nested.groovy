pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo "x ${"in ${1 + 1}"} y"
            }
        }
    }
}
