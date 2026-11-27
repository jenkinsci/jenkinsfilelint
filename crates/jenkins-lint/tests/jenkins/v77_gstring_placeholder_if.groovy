pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo "x ${if (true) { 1 }} y"
            }
        }
    }
}
