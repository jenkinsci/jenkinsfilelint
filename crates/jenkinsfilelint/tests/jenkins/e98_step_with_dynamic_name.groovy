pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                this.("${x}") 'z'
            }
        }
    }
}
