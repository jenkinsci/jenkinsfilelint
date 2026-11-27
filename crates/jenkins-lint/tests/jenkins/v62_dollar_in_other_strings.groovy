pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def a = 'echo $(date) $1'
                    def b = "cost: \$5 and ${a} and $a"
                    def c = /^v\d+$/
                    def d = $/C:\path $ and $$/$
                    def e = """line ${b}
                      $c"""
                }
            }
        }
    }
}
