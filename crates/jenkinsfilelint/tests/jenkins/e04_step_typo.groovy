pipeline {
    agent any
    stages {
        stage('S') {
            step {
                echo 'x'
            }
        }
    }
}
